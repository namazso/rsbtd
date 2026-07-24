// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Introspection-driven `settings get` / `settings set`.
//!
//! The daemon's settings schema is fully typed — one documented GraphQL
//! field per setting — so the CLI discovers field names, value types,
//! and enum values from schema introspection instead of a hardcoded
//! table that could drift. `name=value` pairs keep working: names are
//! accepted in snake_case (the libtorrent spelling) or camelCase, and
//! values are coerced by the introspected input type.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use crate::client::Client;

/// Fetches every type with fields/inputFields/enumValues; the TypeRef
/// fragment is nested deep enough for `[T!]!`.
const INTROSPECTION: &str = "{ __schema { types { \
     name kind enumValues { name } \
     fields { name type { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } \
     inputFields { name type { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } \
     } } }";

/// The daemon's type registry, keyed by type name.
struct Schema {
    types: HashMap<String, Value>,
}

impl Schema {
    async fn fetch(client: &Client) -> Result<Schema, String> {
        let data = client
            .graphql(INTROSPECTION, json!({}))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Schema::from_introspection(&data)
            .ok_or("daemon returned an unexpected introspection response")?)
    }

    fn from_introspection(data: &Value) -> Option<Schema> {
        let types = data["__schema"]["types"]
            .as_array()?
            .iter()
            .filter_map(|t| Some((t["name"].as_str()?.to_owned(), t.clone())))
            .collect();
        Some(Schema { types })
    }

    fn ty(&self, name: &str) -> Result<&Value, String> {
        self.types
            .get(name)
            .ok_or_else(|| format!("daemon schema has no type {name}"))
    }

    /// `(name, type-ref)` for each field of an output object type.
    fn object_fields(&self, type_name: &str) -> Result<Vec<(String, Value)>, String> {
        fields_of(self.ty(type_name)?, "fields")
            .ok_or_else(|| format!("daemon type {type_name} has no fields"))
    }

    /// `(name, type-ref)` for each field of an input object type.
    fn input_fields(&self, type_name: &str) -> Result<Vec<(String, Value)>, String> {
        fields_of(self.ty(type_name)?, "inputFields")
            .ok_or_else(|| format!("daemon type {type_name} has no input fields"))
    }

    /// The selection for one `Settings` field: bare for leaves, a full
    /// recursive sub-selection for object-typed settings.
    fn field_selection(&self, field: &str, type_ref: &Value) -> Result<String, String> {
        let (named, _) = unwrap_type(type_ref);
        if named["kind"] == "OBJECT" {
            let name = named["name"].as_str().unwrap_or_default();
            let subs: Vec<String> = self
                .object_fields(name)?
                .iter()
                .map(|(f, ty)| self.field_selection(f, ty))
                .collect::<Result<_, _>>()?;
            Ok(format!("{field} {{ {} }}", subs.join(" ")))
        } else {
            Ok(field.to_owned())
        }
    }

    /// Coerces a raw command-line value by the introspected input type.
    fn coerce(&self, type_ref: &Value, raw: &str) -> Result<Value, String> {
        if raw == "null" {
            // Meaningful for the nullable groups (proxy, i2p, ...); the
            // daemon rejects it elsewhere with a precise error.
            return Ok(Value::Null);
        }
        let (named, is_list) = unwrap_type(type_ref);
        if is_list {
            let v: Value =
                serde_json::from_str(raw).map_err(|e| format!("expects a JSON array: {e}"))?;
            if !v.is_array() {
                return Err(format!("expects a JSON array, got {raw}"));
            }
            return self.coerce_json(type_ref, v);
        }
        let name = named["name"].as_str().unwrap_or_default();
        match named["kind"].as_str().unwrap_or_default() {
            "SCALAR" => match name {
                "Int" => raw
                    .parse::<i64>()
                    .map(Value::from)
                    .map_err(|_| format!("expects an integer, got {raw}")),
                "Float" => raw
                    .parse::<f64>()
                    .map(Value::from)
                    .map_err(|_| format!("expects a number, got {raw}")),
                "Boolean" => raw
                    .parse::<bool>()
                    .map(Value::Bool)
                    .map_err(|_| format!("expects true or false, got {raw}")),
                _ => Ok(Value::String(raw.to_owned())),
            },
            "ENUM" => self.coerce_enum(name, raw),
            "INPUT_OBJECT" => {
                let v: Value =
                    serde_json::from_str(raw).map_err(|e| format!("expects a JSON object: {e}"))?;
                if !v.is_object() {
                    return Err(format!("expects a JSON object, got {raw}"));
                }
                self.coerce_json(type_ref, v)
            }
            _ => Ok(Value::String(raw.to_owned())),
        }
    }

    /// Matches an enum name case-insensitively, normalizing it to the
    /// schema's spelling.
    fn coerce_enum(&self, type_name: &str, raw: &str) -> Result<Value, String> {
        let values: Vec<String> = self.ty(type_name)?["enumValues"]
            .as_array()
            .map(|vs| {
                vs.iter()
                    .filter_map(|v| v["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        values
            .iter()
            .find(|v| v.eq_ignore_ascii_case(raw))
            .map(|v| Value::String(v.clone()))
            .ok_or_else(|| format!("expects one of {}, got {raw}", values.join(", ")))
    }

    /// Coerces a parsed JSON value by the introspected type, recursing
    /// into lists and input objects: nested keys may be snake_case and
    /// nested enum members are matched case-insensitively.
    fn coerce_json(&self, type_ref: &Value, value: Value) -> Result<Value, String> {
        if value.is_null() {
            return Ok(value);
        }
        let mut t = type_ref;
        while t["kind"] == "NON_NULL" {
            t = &t["ofType"];
        }
        if t["kind"] == "LIST" {
            let Value::Array(items) = value else {
                return Err(format!("expects a JSON array, got {value}"));
            };
            let items = items
                .into_iter()
                .map(|item| self.coerce_json(&t["ofType"], item))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Value::Array(items));
        }
        let name = t["name"].as_str().unwrap_or_default();
        match t["kind"].as_str().unwrap_or_default() {
            "ENUM" => match value {
                Value::String(raw) => self.coerce_enum(name, &raw),
                other => Err(format!("expects a {name} name, got {other}")),
            },
            "INPUT_OBJECT" => {
                let Value::Object(map) = value else {
                    return Err(format!("expects a JSON object, got {value}"));
                };
                let fields = self.input_fields(name)?;
                let mut out = Map::new();
                for (key, member) in map {
                    let camel = to_camel(&key);
                    let (_, ty) = fields
                        .iter()
                        .find(|(f, _)| *f == camel)
                        .ok_or_else(|| format!("unknown field {key} in {name}"))?;
                    let member = self
                        .coerce_json(ty, member)
                        .map_err(|e| format!("{key}: {e}"))?;
                    out.insert(camel, member);
                }
                Ok(Value::Object(out))
            }
            _ => Ok(value),
        }
    }
}

fn fields_of(ty: &Value, key: &str) -> Option<Vec<(String, Value)>> {
    Some(
        ty[key]
            .as_array()?
            .iter()
            .filter_map(|f| Some((f["name"].as_str()?.to_owned(), f["type"].clone())))
            .collect(),
    )
}

/// Strips NON_NULL/LIST wrappers; returns the named type-ref and whether
/// a list wrapper was seen.
fn unwrap_type(type_ref: &Value) -> (&Value, bool) {
    let mut t = type_ref;
    let mut is_list = false;
    while matches!(t["kind"].as_str(), Some("NON_NULL" | "LIST")) {
        is_list |= t["kind"] == "LIST";
        match t.get("ofType") {
            Some(inner) if !inner.is_null() => t = inner,
            _ => break,
        }
    }
    (t, is_list)
}

/// snake_case (or mixed) → the schema's camelCase field names.
fn to_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// camelCase → snake_case, the spelling used for CLI output (matches the
/// libtorrent setting names).
fn to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `settings get [names...]` — prints `name = <json>` per setting.
pub async fn get(client: &Client, names: &[String], json_output: bool) -> Result<(), String> {
    let schema = Schema::fetch(client).await?;
    let fields = schema.object_fields("Settings")?;
    let selected: Vec<(String, Value)> = if names.is_empty() {
        fields
    } else {
        names
            .iter()
            .map(|name| {
                let camel = to_camel(name);
                fields
                    .iter()
                    .find(|(f, _)| *f == camel)
                    .map(|(f, ty)| (f.clone(), ty.clone()))
                    .ok_or_else(|| format!("unknown setting: {name}"))
            })
            .collect::<Result<_, _>>()?
    };
    let selection: Vec<String> = selected
        .iter()
        .map(|(f, ty)| schema.field_selection(f, ty))
        .collect::<Result<_, _>>()?;
    let data = client
        .graphql(
            &format!("{{ settings {{ {} }} }}", selection.join(" ")),
            json!({}),
        )
        .await
        .map_err(|e| e.to_string())?;
    print_settings(
        &data["settings"],
        selected.iter().map(|(f, _)| f),
        json_output,
    );
    Ok(())
}

/// `settings set name=value...` — applies one atomic delta and prints
/// the new effective values of the touched settings.
pub async fn set(client: &Client, assignments: &[String], json_output: bool) -> Result<(), String> {
    if assignments.is_empty() {
        return Err("nothing to set: pass name=value pairs".to_owned());
    }
    let schema = Schema::fetch(client).await?;
    let fields = schema.input_fields("SettingsInput")?;
    let mut input = Map::new();
    let mut touched = Vec::new();
    for assignment in assignments {
        let (name, raw) = assignment
            .split_once('=')
            .ok_or_else(|| format!("bad assignment {assignment}: expected name=value"))?;
        let camel = to_camel(name);
        let (_, ty) = fields
            .iter()
            .find(|(f, _)| *f == camel)
            .ok_or_else(|| format!("unknown setting: {name}"))?;
        let value = schema.coerce(ty, raw).map_err(|e| format!("{name}: {e}"))?;
        input.insert(camel.clone(), value);
        touched.push(camel);
    }
    let output_fields = schema.object_fields("Settings")?;
    let selection: Vec<String> = touched
        .iter()
        .map(|f| {
            let (_, ty) = output_fields
                .iter()
                .find(|(name, _)| name == f)
                .ok_or_else(|| format!("daemon schema has no readback field {f}"))?;
            schema.field_selection(f, ty)
        })
        .collect::<Result<_, _>>()?;
    let data = client
        .graphql(
            &format!(
                "mutation($input: SettingsInput!) {{ applySettings(input: $input) {{ {} }} }}",
                selection.join(" ")
            ),
            json!({ "input": Value::Object(input) }),
        )
        .await
        .map_err(|e| e.to_string())?;
    print_settings(&data["applySettings"], touched.iter(), json_output);
    Ok(())
}

fn print_settings<'a>(data: &Value, fields: impl Iterator<Item = &'a String>, json_output: bool) {
    if json_output {
        println!("{data:#}");
    } else {
        for field in fields {
            println!("{} = {}", to_snake(field), data[field.as_str()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_normalization_roundtrips() {
        for (snake, camel) in [
            ("upload_rate_limit", "uploadRateLimit"),
            ("enable_dht", "enableDht"),
            ("i2p", "i2p"),
            ("proxy", "proxy"),
            (
                "socks5_udp_send_local_endpoint",
                "socks5UdpSendLocalEndpoint",
            ),
        ] {
            assert_eq!(to_camel(snake), camel);
            assert_eq!(to_snake(camel), snake);
            // camelCase input is accepted as-is.
            assert_eq!(to_camel(camel), camel);
        }
    }

    fn test_schema() -> Schema {
        Schema::from_introspection(&json!({
            "__schema": { "types": [
                {
                    "name": "Settings", "kind": "OBJECT",
                    "fields": [
                        { "name": "uploadRateLimit",
                          "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "Int" } } },
                        { "name": "proxy",
                          "type": { "kind": "OBJECT", "name": "ProxySettings" } },
                    ],
                },
                {
                    "name": "ProxySettings", "kind": "OBJECT",
                    "fields": [
                        { "name": "protocol",
                          "type": { "kind": "NON_NULL", "ofType": { "kind": "ENUM", "name": "ProxyProtocol" } } },
                        { "name": "port",
                          "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "Int" } } },
                    ],
                },
                {
                    "name": "SettingsInput", "kind": "INPUT_OBJECT",
                    "inputFields": [
                        { "name": "uploadRateLimit", "type": { "kind": "SCALAR", "name": "Int" } },
                        { "name": "enableDht", "type": { "kind": "SCALAR", "name": "Boolean" } },
                        { "name": "userAgent", "type": { "kind": "ENUM", "name": "UserAgent" } },
                        { "name": "outgoingInterfaces",
                          "type": { "kind": "LIST", "ofType": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "String" } } } },
                        { "name": "outgoingPortRange",
                          "type": { "kind": "INPUT_OBJECT", "name": "PortRangeInput" } },
                        { "name": "proxy",
                          "type": { "kind": "INPUT_OBJECT", "name": "ProxyInput" } },
                        { "name": "listenInterfaces",
                          "type": { "kind": "LIST", "ofType": { "kind": "NON_NULL", "ofType": { "kind": "INPUT_OBJECT", "name": "ListenInterfaceInput" } } } },
                    ],
                },
                {
                    "name": "UserAgent", "kind": "ENUM",
                    "enumValues": [ { "name": "NONE" }, { "name": "RSBTD" }, { "name": "QBITTORRENT" } ],
                },
                {
                    "name": "PortRangeInput", "kind": "INPUT_OBJECT",
                    "inputFields": [
                        { "name": "first", "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "Int" } } },
                        { "name": "last", "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "Int" } } },
                    ],
                },
                {
                    "name": "ProxyInput", "kind": "INPUT_OBJECT",
                    "inputFields": [
                        { "name": "protocol", "type": { "kind": "NON_NULL", "ofType": { "kind": "ENUM", "name": "ProxyProtocol" } } },
                        { "name": "resolveHostnames", "type": { "kind": "SCALAR", "name": "Boolean" } },
                    ],
                },
                {
                    "name": "ProxyProtocol", "kind": "ENUM",
                    "enumValues": [ { "name": "SOCKS5" }, { "name": "HTTP" } ],
                },
                {
                    "name": "ListenInterfaceInput", "kind": "INPUT_OBJECT",
                    "inputFields": [
                        { "name": "interface", "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "String" } } },
                        { "name": "port", "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "Int" } } },
                        { "name": "ssl", "type": { "kind": "SCALAR", "name": "Boolean" } },
                    ],
                },
            ] }
        }))
        .expect("test schema")
    }

    fn input_type(schema: &Schema, field: &str) -> Value {
        schema
            .input_fields("SettingsInput")
            .unwrap()
            .into_iter()
            .find(|(f, _)| f == field)
            .map(|(_, ty)| ty)
            .unwrap()
    }

    #[test]
    fn coerces_by_introspected_type() {
        let schema = test_schema();
        let int = input_type(&schema, "uploadRateLimit");
        assert_eq!(schema.coerce(&int, "123000").unwrap(), json!(123000));
        assert!(schema.coerce(&int, "fast").unwrap_err().contains("integer"));

        let boolean = input_type(&schema, "enableDht");
        assert_eq!(schema.coerce(&boolean, "false").unwrap(), json!(false));
        assert!(
            schema
                .coerce(&boolean, "yes")
                .unwrap_err()
                .contains("true or false")
        );

        // Enums are case-insensitive and normalized to the schema name.
        let ua = input_type(&schema, "userAgent");
        assert_eq!(schema.coerce(&ua, "rsbtd").unwrap(), json!("RSBTD"));
        assert_eq!(
            schema.coerce(&ua, "qBittorrent").unwrap(),
            json!("QBITTORRENT")
        );
        assert!(
            schema
                .coerce(&ua, "netscape")
                .unwrap_err()
                .contains("RSBTD")
        );

        // Lists and objects take JSON; keys may be snake_case.
        let list = input_type(&schema, "outgoingInterfaces");
        assert_eq!(
            schema.coerce(&list, r#"["eth0"]"#).unwrap(),
            json!(["eth0"])
        );
        assert!(schema.coerce(&list, "eth0").unwrap_err().contains("array"));

        let range = input_type(&schema, "outgoingPortRange");
        assert_eq!(
            schema
                .coerce(&range, r#"{"first":6900,"last":6910}"#)
                .unwrap(),
            json!({"first": 6900, "last": 6910})
        );
        assert_eq!(schema.coerce(&range, "null").unwrap(), Value::Null);
    }

    /// Enum members and snake_case keys nested inside structured
    /// settings are coerced by the introspected member types.
    #[test]
    fn coerces_nested_structured_members() {
        let schema = test_schema();

        let proxy = input_type(&schema, "proxy");
        assert_eq!(
            schema
                .coerce(&proxy, r#"{"protocol":"socks5","resolve_hostnames":true}"#)
                .unwrap(),
            json!({"protocol": "SOCKS5", "resolveHostnames": true})
        );
        assert!(
            schema
                .coerce(&proxy, r#"{"protocol":"gopher"}"#)
                .unwrap_err()
                .contains("SOCKS5")
        );
        assert!(
            schema
                .coerce(&proxy, r#"{"protocol":"http","password":"x"}"#)
                .unwrap_err()
                .contains("unknown field password")
        );
        // Explicit nulls pass through for the daemon to validate.
        assert_eq!(
            schema
                .coerce(&proxy, r#"{"protocol":"http","resolve_hostnames":null}"#)
                .unwrap(),
            json!({"protocol": "HTTP", "resolveHostnames": null})
        );

        // Objects nested inside lists are coerced element-wise.
        let listen = input_type(&schema, "listenInterfaces");
        assert_eq!(
            schema
                .coerce(&listen, r#"[{"interface":"[::]","port":6881,"ssl":true}]"#)
                .unwrap(),
            json!([{"interface": "[::]", "port": 6881, "ssl": true}])
        );
        assert!(
            schema
                .coerce(&listen, r#"[{"iface":"eth0"}]"#)
                .unwrap_err()
                .contains("unknown field iface")
        );
    }

    #[test]
    fn builds_recursive_selections() {
        let schema = test_schema();
        let fields = schema.object_fields("Settings").unwrap();
        let (name, ty) = &fields[0];
        assert_eq!(schema.field_selection(name, ty).unwrap(), "uploadRateLimit");
        let (name, ty) = &fields[1];
        assert_eq!(
            schema.field_selection(name, ty).unwrap(),
            "proxy { protocol port }"
        );
    }
}
