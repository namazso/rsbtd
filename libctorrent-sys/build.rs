// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir.parent().unwrap().to_path_buf();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=wrapper.h");
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("libctorrent").display()
    );
    println!("cargo:rerun-if-env-changed=CTORRENT_LIBTORRENT_PREFIX");
    println!("cargo:rerun-if-env-changed=CTORRENT_VENDORED_DEPRECATED");
    println!("cargo:rerun-if-env-changed=OPENSSL_ROOT_DIR");

    // 1. Resolve libtorrent: explicit prefix > vendored build > system.
    let lt_prefix: Option<PathBuf> = match env::var("CTORRENT_LIBTORRENT_PREFIX") {
        Ok(p) if !p.is_empty() => {
            if env::var_os("CARGO_FEATURE_VENDORED").is_some() {
                println!(
                    "cargo:warning=CTORRENT_LIBTORRENT_PREFIX overrides the \
                     `vendored` feature; linking against {p}"
                );
            }
            Some(PathBuf::from(p))
        }
        _ if env::var_os("CARGO_FEATURE_VENDORED").is_some() => {
            Some(build_vendored_libtorrent(&repo_root, &out_dir))
        }
        _ => None,
    };

    // 2. Build libctorrent against it. The dedicated CMake variable makes
    // the prefix a strict override: libctorrent resolves libtorrent from
    // exactly there (no default paths, no pkg-config fallback) and fails
    // the configure step on a miss instead of silently picking a system
    // libtorrent.
    let mut cfg = cmake::Config::new(repo_root.join("libctorrent"));
    cfg.out_dir(out_dir.join("ctorrent"));
    force_non_debug_crt(&mut cfg);
    if let Some(ref prefix) = lt_prefix {
        cfg.define("CTORRENT_LIBTORRENT_PREFIX", prefix);
    }
    let dst = cfg.build();

    println!(
        "cargo:rustc-link-search=native={}",
        dst.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=ctorrent");

    // 3. Link the libtorrent the shim was configured against. The vendored
    // build is a static library and gets absorbed into the Rust binary; a
    // system or prefix libtorrent links however it was built.
    let manifest = dst.join("lib/ctorrent-link.txt");
    let link = parse_link_manifest(&manifest);
    if !link.dir.is_empty() {
        println!("cargo:rustc-link-search=native={}", link.dir);
    }
    if link.is_static {
        println!("cargo:rustc-link-lib=static={}", link.lib);
        // A static archive does not carry its dependencies; link the ones
        // libtorrent uses (Boost is header-only at 2.1).
        if win_msvc() {
            if let Some(openssl) = openssl_root() {
                println!(
                    "cargo:rustc-link-search=native={}",
                    openssl.join("lib").display()
                );
                println!("cargo:rustc-link-lib=static=libssl");
                println!("cargo:rustc-link-lib=static=libcrypto");
            }
            // libtorrent's PUBLIC system libraries plus what static OpenSSL
            // needs (crypt32, advapi32, user32, gdi32).
            for lib in [
                "ws2_32", "iphlpapi", "bcrypt", "mswsock", "crypt32", "advapi32", "user32", "gdi32",
            ] {
                println!("cargo:rustc-link-lib=dylib={lib}");
            }
        } else {
            println!("cargo:rustc-link-lib=dylib=ssl");
            println!("cargo:rustc-link-lib=dylib=crypto");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib={}", link.lib);
        // The shim's objects instantiate libtorrent header code that calls
        // into OpenSSL (error categories); with --as-needed a shared
        // libtorrent's own dependency on libcrypto cannot satisfy those
        // direct references, so link OpenSSL here as well.
        if !win_msvc() {
            println!("cargo:rustc-link-lib=dylib=ssl");
            println!("cargo:rustc-link-lib=dylib=crypto");
        }
    }
    // Dependency link items discovered alongside libtorrent (WebTorrent's
    // datachannel/usrsctp, Apple frameworks, ...).
    for item in &link.extra {
        emit_extra_link_item(item);
    }

    // MSVC objects request their C++ runtime via /DEFAULTLIB directives;
    // elsewhere the C++ standard library must be linked explicitly.
    if !win_msvc() {
        match env::var("CARGO_CFG_TARGET_OS").unwrap_or_default().as_str() {
            "macos" => println!("cargo:rustc-link-lib=dylib=c++"),
            _ => println!("cargo:rustc-link-lib=dylib=stdc++"),
        }
    }

    // 4. Generate the bindings from the installed headers (which include the
    // configure-time generated ct_abi_config.h).
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", dst.join("include").display()))
        .allowlist_function("ct_.*")
        .allowlist_type("ct_.*")
        .allowlist_var("CT_.*")
        .prepend_enum_name(false)
        .derive_default(true)
        // Masquerade storage types own C++ state (weak_ptr/shared_ptr) under
        // a clone/drop protocol; a bitwise Copy would alias that ownership
        // and double-destroy it, and a zeroed Default is not a valid object.
        .no_copy("ct_torrent_handle")
        .no_copy("ct_session_proxy")
        .no_default("ct_torrent_handle")
        .no_default("ct_session_proxy")
        // By-value owning types (heap allocation behind `box_`, released by
        // the matching *_free): receiving one transfers ownership, so a
        // bitwise Copy would alias the allocation and double-free it. Their
        // zeroed Default stays valid (empty, freeing it is a no-op).
        .no_copy("ct_str")
        .no_copy("ct_buf")
        .no_copy("ct_file_slice_array")
        .layout_tests(true)
        .generate()
        .expect("bindgen failed on libctorrent headers");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}

fn build_vendored_libtorrent(repo_root: &Path, out_dir: &Path) -> PathBuf {
    let src = repo_root.join("vendor/libtorrent");
    assert!(
        src.join("CMakeLists.txt").exists(),
        "vendor/libtorrent is missing; run: git submodule update --init --recursive"
    );
    assert!(
        src.join("deps/try_signal/CMakeLists.txt").exists(),
        "vendor/libtorrent/deps are missing; run: git submodule update --init --recursive"
    );
    // The parts of the vendored tree that feed the build (not docs/tests),
    // so source changes invalidate the cached libtorrent.
    for input in ["CMakeLists.txt", "cmake", "deps", "include", "src"] {
        println!("cargo:rerun-if-changed={}", src.join(input).display());
    }
    // Match the distro norm (deprecated-functions=ON => TORRENT_ABI_VERSION=2)
    // unless overridden; CTORRENT_VENDORED_DEPRECATED=OFF builds at ABI 100
    // (used by the CI ABI matrix).
    let deprecated = match env::var("CTORRENT_VENDORED_DEPRECATED").as_deref() {
        Ok("OFF") | Ok("off") | Ok("0") => "OFF",
        _ => "ON",
    };
    let mut cfg = cmake::Config::new(&src);
    cfg.out_dir(out_dir.join("libtorrent"))
        // Static: the vendored libtorrent is absorbed into the Rust binary.
        .define("BUILD_SHARED_LIBS", "OFF")
        // Rust binaries are PIE by default; the archive must be PIC.
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .define("deprecated-functions", deprecated)
        // Avoids the libdatachannel submodule tree.
        .define("webtorrent", "OFF")
        .define("build_tests", "OFF")
        .define("build_examples", "OFF")
        .define("python-bindings", "OFF");
    force_non_debug_crt(&mut cfg);
    if win_msvc() {
        // Make the OpenSSL state deterministic: either link the static
        // OpenSSL named by OPENSSL_ROOT_DIR (windows/build.ps1 sets it), or
        // build without OpenSSL entirely. Never let CMake pick up a stray
        // OpenSSL that rustc then fails to link.
        match openssl_root() {
            Some(root) => {
                cfg.define("OPENSSL_ROOT_DIR", &root);
                cfg.define("OPENSSL_USE_STATIC_LIBS", "ON");
            }
            None => {
                println!(
                    "cargo:warning=OPENSSL_ROOT_DIR is not set; building libtorrent \
                     without OpenSSL (no encrypted peer connections, no HTTPS trackers)"
                );
                cfg.define("CMAKE_DISABLE_FIND_PACKAGE_OpenSSL", "TRUE");
            }
        }
    }
    cfg.build()
}

fn win_msvc() -> bool {
    env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows"
        && env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "msvc"
}

fn openssl_root() -> Option<PathBuf> {
    match env::var("OPENSSL_ROOT_DIR") {
        Ok(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => None,
    }
}

/// On MSVC a CMake `Debug` configuration selects the debug CRT (/MDd),
/// which cannot be mixed with the release CRT rustc always links. Pin
/// debug cargo builds to RelWithDebInfo; release builds already map to a
/// non-debug configuration.
fn force_non_debug_crt(cfg: &mut cmake::Config) {
    if win_msvc() && env::var("PROFILE").as_deref() == Ok("debug") {
        cfg.profile("RelWithDebInfo");
    }
}

struct LinkManifest {
    dir: String,
    lib: String,
    is_static: bool,
    /// Additional resolved link items (dependency archives/libraries,
    /// `-l`/`-framework` flags) beyond the primary library.
    extra: Vec<String>,
}

fn parse_link_manifest(path: &Path) -> LinkManifest {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut link = LinkManifest {
        dir: String::new(),
        lib: String::new(),
        is_static: false,
        extra: Vec::new(),
    };
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("LINKDIR=") {
            link.dir = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("LIB=") {
            link.lib = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("TYPE=") {
            link.is_static = v.trim() == "STATIC_LIBRARY";
        } else if let Some(v) = line.strip_prefix("EXTRA=") {
            let v = v.trim();
            if !v.is_empty() {
                link.extra.push(v.to_string());
            }
        }
    }
    assert!(!link.lib.is_empty(), "malformed {}", path.display());
    link
}

/// Emits the cargo link directives for one EXTRA manifest item: a
/// `-framework`/`-l` flag, an absolute archive or shared library path,
/// or a bare library name.
fn emit_extra_link_item(item: &str) {
    if let Some(framework) = item.strip_prefix("-framework ") {
        println!("cargo:rustc-link-lib=framework={}", framework.trim());
        return;
    }
    if let Some(name) = item.strip_prefix("-l") {
        println!("cargo:rustc-link-lib=dylib={name}");
        return;
    }
    if item.starts_with('-') {
        println!("cargo:rustc-link-arg={item}");
        return;
    }
    let path = Path::new(item);
    if path.is_absolute() {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext == "a" || ext == "lib" {
            if let Some(dir) = path.parent() {
                println!("cargo:rustc-link-search=native={}", dir.display());
            }
            // Only Unix `libfoo.a` archives drop the prefix (rustc rebuilds
            // it for `-lfoo`); an MSVC `libssl.lib` resolves by its full
            // stem, so stripping would ask the linker for `ssl.lib`.
            let name = if ext == "a" {
                stem.strip_prefix("lib").unwrap_or(stem)
            } else {
                stem
            };
            println!("cargo:rustc-link-lib=static={name}");
        } else {
            // Shared libraries may be versioned (libfoo.so.3); pass the
            // path straight to the linker.
            println!("cargo:rustc-link-arg={item}");
        }
        return;
    }
    println!("cargo:rustc-link-lib=dylib={item}");
}
