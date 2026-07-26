// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! rsbtctl — oneshot command-line control client for rsbtd.

mod client;
#[cfg(windows)]
mod registry;
mod settings_cli;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};

use client::Client;
#[cfg(unix)]
use client::Target;

#[derive(Parser)]
#[command(
    name = "rsbtctl",
    version,
    about = "Control client for the rsbtd torrent daemon"
)]
struct Cli {
    /// Daemon URL, e.g. http://127.0.0.1:3928 or https://host (via a
    /// TLS reverse proxy). On Windows, defaults to the installed
    /// daemon's registry configuration (address and token).
    #[arg(long, global = true)]
    #[cfg_attr(unix, arg(conflicts_with = "unix"))]
    url: Option<String>,

    /// Daemon unix socket path.
    #[cfg(unix)]
    #[arg(long, global = true)]
    unix: Option<PathBuf>,

    /// API bearer token (or set RSBTCTL_TOKEN).
    #[arg(long, global = true, env = "RSBTCTL_TOKEN", hide_env_values = true)]
    token: Option<String>,

    /// Print raw JSON instead of human-readable output.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Daemon and libtorrent versions.
    Version,
    /// Session state (ports, DHT, pause, torrent count).
    Session,
    /// List torrents.
    List {
        /// Only torrents in this state (e.g. seeding, downloading).
        #[arg(long)]
        state: Option<String>,
    },
    /// Detailed status of one torrent.
    Status(StatusArgs),
    /// Add a torrent from a magnet link or a .torrent file.
    Add(AddArgs),
    /// Remove a torrent.
    Remove {
        uuid: String,
        /// Also delete the downloaded files.
        #[arg(long)]
        delete_files: bool,
    },
    /// Pause a torrent (detaches it from auto-management).
    Pause { uuid: String },
    /// Resume a paused torrent.
    Resume { uuid: String },
    /// Pause the whole session.
    SessionPause,
    /// Resume the whole session.
    SessionResume,
    /// Wait until a torrent reaches a condition.
    Wait {
        uuid: String,
        /// metadata, finished, or seeding.
        #[arg(long)]
        until: WaitUntil,
        /// Give up after this many seconds.
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Read or change libtorrent settings.
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
    /// Session statistics counters.
    Stats {
        /// Only these metric names.
        names: Vec<String>,
    },
    /// Create a .torrent from a local file or directory.
    Create(CreateArgs),
}

#[derive(Args)]
struct StatusArgs {
    /// Torrent uuid.
    #[arg(
        required_unless_present_any = ["hash_v1", "hash_v2"],
        conflicts_with_all = ["hash_v1", "hash_v2"],
    )]
    uuid: Option<String>,
    /// Look the torrent up by its v1 (SHA-1) info-hash: 40 hex characters.
    #[arg(long, conflicts_with = "hash_v2")]
    hash_v1: Option<String>,
    /// Look the torrent up by its v2 (SHA-256) info-hash: 64 hex characters.
    #[arg(long)]
    hash_v2: Option<String>,
}

#[derive(Args)]
struct AddArgs {
    /// Magnet link.
    #[arg(long, conflicts_with = "file", required_unless_present = "file")]
    magnet: Option<String>,
    /// Path to a .torrent file (read locally, uploaded as base64).
    #[arg(long)]
    file: Option<PathBuf>,
    /// Download directory (daemon-local).
    #[arg(long)]
    save_path: String,
    /// Add paused.
    #[arg(long)]
    paused: bool,
    /// Download pieces in order.
    #[arg(long)]
    sequential: bool,
    /// Assume the data is already complete (seed immediately).
    #[arg(long)]
    seed_mode: bool,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum WaitUntil {
    Metadata,
    Finished,
    Seeding,
}

#[derive(Subcommand)]
enum SettingsCommand {
    /// Print settings (all, or the given names).
    Get { names: Vec<String> },
    /// Change settings, e.g. `upload_rate_limit=100000 enable_dht=false`.
    /// Enum settings take a name (`user_agent=rsbtd`); structured
    /// settings take JSON, e.g. `proxy=null` or
    /// `outgoing_port_range={"first":6900,"last":6910}`.
    Set { assignments: Vec<String> },
}

#[derive(Args)]
struct CreateArgs {
    /// The file or directory to create the torrent from (daemon-local).
    path: String,
    /// Write the .torrent here (daemon-local). Without it, the torrent is
    /// returned inline and written next to the current directory.
    #[arg(long)]
    out: Option<String>,
    /// Tracker announce URL (repeatable).
    #[arg(long = "tracker")]
    trackers: Vec<String>,
    /// Mark the torrent private (BEP 27).
    #[arg(long)]
    private: bool,
    /// Comment stored in the torrent.
    #[arg(long)]
    comment: Option<String>,
    /// Only start the job and print its id; don't wait for hashing.
    #[arg(long)]
    no_wait: bool,
    /// Overwrite an existing local .torrent file (without --out, the
    /// result is written next to the current directory and existing
    /// files are refused by default).
    #[arg(long)]
    force: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("cannot start async runtime");
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("rsbtctl: {message}");
            ExitCode::FAILURE
        }
    }
}

fn make_client(cli: &Cli) -> Result<Client, String> {
    #[cfg(unix)]
    if let Some(path) = &cli.unix {
        return Ok(Client::new(Target::Unix(path.clone()), cli.token.clone()));
    }
    match &cli.url {
        Some(url) => Ok(Client::new(client::parse_url(url)?, cli.token.clone())),
        None => {
            // The tray app keeps the daemon's address and token in the
            // registry; use them so the installed daemon is reachable
            // without flags. An explicit --token still wins.
            #[cfg(windows)]
            if let Some((url, token)) = registry::daemon_defaults()? {
                let token = cli.token.clone().or(token);
                return Ok(Client::new(client::parse_url(&url)?, token));
            }
            Err(if cfg!(unix) {
                "pass --url or --unix to select the daemon".to_owned()
            } else {
                "pass --url to select the daemon (no installed rsbtd configuration found)"
                    .to_owned()
            })
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let client = make_client(&cli)?;
    let json_output = cli.json;
    let print = |data: &Value, human: &dyn Fn(&Value) -> String| {
        if json_output {
            println!("{data:#}");
        } else {
            println!("{}", human(data));
        }
    };

    let gql = |query: &'static str, variables: Value| {
        let client = &client;
        async move {
            client
                .graphql(query, variables)
                .await
                .map_err(|e| e.to_string())
        }
    };

    match cli.command {
        Command::Version => {
            let data = gql("{ version { daemon libtorrent } }", json!({})).await?;
            print(&data, &|d| {
                format!(
                    "rsbtd {} (libtorrent {})",
                    field_str(&d["version"]["daemon"]),
                    field_str(&d["version"]["libtorrent"])
                )
            });
        }
        Command::Session => {
            let data = gql(
                "{ session { isPaused isListening isDhtRunning listenPort torrentCount } }",
                json!({}),
            )
            .await?;
            print(&data, &|d| {
                let s = &d["session"];
                format!(
                    "listening: {} (port {})\ndht: {}\npaused: {}\ntorrents: {}",
                    s["isListening"],
                    s["listenPort"],
                    s["isDhtRunning"],
                    s["isPaused"],
                    s["torrentCount"]
                )
            });
        }
        Command::List { state } => {
            let state = state.map(|s| s.to_uppercase());
            let data = gql(
                "query($state: TorrentState) { torrents(state: $state) { \
                   uuid name state progressPpm downloadRate uploadRate totalDone totalSize } }",
                json!({ "state": state }),
            )
            .await?;
            print(&data, &|d| {
                let torrents = d["torrents"].as_array().cloned().unwrap_or_default();
                if torrents.is_empty() {
                    return "no torrents".to_owned();
                }
                torrents
                    .iter()
                    .map(|t| {
                        format!(
                            "{}  {:<20} {:>6.1}%  {}",
                            field_str(&t["uuid"]),
                            field_str(&t["state"]),
                            t["progressPpm"].as_f64().unwrap_or(0.0) / 10_000.0,
                            field_str(&t["name"]),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::Status(args) => {
            const FIELDS: &str = "name state progressPpm savePath totalSize totalDone \
                 downloadRate uploadRate numPeers numSeeds isPaused isFinished \
                 isSeeding hasMetadata magnetUri flags error { message }";
            // The hash lookups are aliased back to `torrent`, so the
            // response shape is the same whichever key was given.
            let (query, variables, ident) = match (args.uuid, args.hash_v1, args.hash_v2) {
                (Some(uuid), ..) => (
                    format!("query($k: UUID!) {{ torrent(uuid: $k) {{ {FIELDS} }} }}"),
                    json!({ "k": uuid }),
                    uuid,
                ),
                (_, Some(hash), _) => (
                    format!(
                        "query($k: Sha1Sum!) {{ torrent: torrentByHashV1(hash: $k) {{ {FIELDS} }} }}"
                    ),
                    json!({ "k": hash }),
                    hash,
                ),
                (_, _, Some(hash)) => (
                    format!(
                        "query($k: Sha256Sum!) {{ torrent: torrentByHashV2(hash: $k) {{ {FIELDS} }} }}"
                    ),
                    json!({ "k": hash }),
                    hash,
                ),
                // clap requires the uuid unless a hash flag is present.
                (None, None, None) => unreachable!("status takes a uuid or a hash"),
            };
            let data = client
                .graphql(&query, variables)
                .await
                .map_err(|e| e.to_string())?;
            if data["torrent"].is_null() {
                return Err(format!("torrent {ident} not found"));
            }
            print(&data, &|d| {
                let t = &d["torrent"];
                let mut out = format!(
                    "name: {}\nstate: {} ({:.1}%)\nsave path: {}\ndone: {} / {} bytes\n\
                     rates: down {} B/s, up {} B/s\npeers: {} ({} seeds)\nflags: {}",
                    field_str(&t["name"]),
                    field_str(&t["state"]),
                    t["progressPpm"].as_f64().unwrap_or(0.0) / 10_000.0,
                    field_str(&t["savePath"]),
                    t["totalDone"],
                    t["totalSize"],
                    t["downloadRate"],
                    t["uploadRate"],
                    t["numPeers"],
                    t["numSeeds"],
                    t["flags"]
                        .as_array()
                        .map(|f| f.iter().map(field_str).collect::<Vec<_>>().join(","))
                        .unwrap_or_default(),
                );
                if let Some(message) = t["error"]["message"].as_str() {
                    out.push_str(&format!("\nerror: {message}"));
                }
                out
            });
        }
        Command::Add(args) => {
            let mut input = json!({ "savePath": args.save_path });
            if let Some(magnet) = args.magnet {
                input["magnetUri"] = magnet.into();
            } else if let Some(file) = args.file {
                let bytes = std::fs::read(&file)
                    .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
                input["torrentData"] = BASE64.encode(bytes).into();
            }
            if args.paused {
                input["paused"] = true.into();
            }
            if args.sequential {
                input["sequentialDownload"] = true.into();
            }
            if args.seed_mode {
                input["flags"] = json!(["SEED_MODE"]);
            }
            let data = gql(
                "mutation($input: AddTorrentInput!) { addTorrent(input: $input) { uuid name } }",
                json!({ "input": input }),
            )
            .await?;
            print(&data, &|d| {
                format!(
                    "added {} ({})",
                    field_str(&d["addTorrent"]["uuid"]),
                    field_str(&d["addTorrent"]["name"])
                )
            });
        }
        Command::Remove { uuid, delete_files } => {
            let data = gql(
                "mutation($u: UUID!, $d: Boolean!) { removeTorrent(uuid: $u, deleteFiles: $d) }",
                json!({ "u": uuid, "d": delete_files }),
            )
            .await?;
            print(&data, &|_| "removed".to_owned());
        }
        Command::Pause { uuid } => {
            let data = gql(
                "mutation($u: UUID!) { pauseTorrent(uuid: $u) }",
                json!({ "u": uuid }),
            )
            .await?;
            print(&data, &|_| "paused".to_owned());
        }
        Command::Resume { uuid } => {
            let data = gql(
                "mutation($u: UUID!) { resumeTorrent(uuid: $u) }",
                json!({ "u": uuid }),
            )
            .await?;
            print(&data, &|_| "resumed".to_owned());
        }
        Command::SessionPause => {
            let data = gql("mutation { pauseSession }", json!({})).await?;
            print(&data, &|_| "session paused".to_owned());
        }
        Command::SessionResume => {
            let data = gql("mutation { resumeSession }", json!({})).await?;
            print(&data, &|_| "session resumed".to_owned());
        }
        Command::Wait {
            uuid,
            until,
            timeout,
        } => {
            let deadline = Instant::now() + Duration::from_secs(timeout);
            loop {
                let data = gql(
                    "query($u: UUID!) { torrent(uuid: $u) { hasMetadata isFinished isSeeding } }",
                    json!({ "u": uuid }),
                )
                .await?;
                let t = &data["torrent"];
                if t.is_null() {
                    return Err(format!("torrent {uuid} not found"));
                }
                let done = match until {
                    WaitUntil::Metadata => t["hasMetadata"] == true,
                    WaitUntil::Finished => t["isFinished"] == true,
                    WaitUntil::Seeding => t["isSeeding"] == true,
                };
                if done {
                    if json_output {
                        println!("{data:#}");
                    } else {
                        println!("done");
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(format!("timed out after {timeout}s"));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        Command::Settings { command } => match command {
            SettingsCommand::Get { names } => {
                settings_cli::get(&client, &names, json_output).await?;
            }
            SettingsCommand::Set { assignments } => {
                settings_cli::set(&client, &assignments, json_output).await?;
            }
        },
        Command::Stats { names } => {
            let names = if names.is_empty() {
                Value::Null
            } else {
                json!(names)
            };
            let data = gql(
                "query($names: [String!]) { sessionStats(names: $names) { name value } }",
                json!({ "names": names }),
            )
            .await?;
            print(&data, &|d| {
                d["sessionStats"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|s| format!("{} = {}", field_str(&s["name"]), s["value"]))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::Create(args) => {
            let mut input = json!({
                "sourcePath": args.path,
                "private": args.private,
            });
            if !args.trackers.is_empty() {
                input["trackers"] = args
                    .trackers
                    .iter()
                    .map(|url| json!({ "url": url }))
                    .collect();
            }
            if let Some(comment) = &args.comment {
                input["comment"] = comment.clone().into();
            }
            if let Some(out) = &args.out {
                input["outputPath"] = out.clone().into();
            }
            let data = gql(
                "mutation($input: CreateTorrentInput!) { startCreateTorrent(input: $input) { id } }",
                json!({ "input": input }),
            )
            .await?;
            let id = data["startCreateTorrent"]["id"]
                .as_u64()
                .ok_or("daemon returned no job id")?;
            if args.no_wait {
                print(&data, &|_| format!("started creation job {id}"));
                return Ok(());
            }
            loop {
                let data = gql(
                    "query($id: Int!) { createJob(id: $id) { state piecesDone piecesTotal error torrentData outputPath } }",
                    json!({ "id": id }),
                )
                .await?;
                let job = &data["createJob"];
                match job["state"].as_str().unwrap_or_default() {
                    "FINISHED" => {
                        if let Some(path) = job["outputPath"].as_str() {
                            print(&data, &|_| format!("created {path}"));
                        } else {
                            let bytes = BASE64
                                .decode(job["torrentData"].as_str().unwrap_or_default())
                                .map_err(|e| format!("bad torrent data: {e}"))?;
                            let name = PathBuf::from(&args.path);
                            let file = format!(
                                "{}.torrent",
                                name.file_name().unwrap_or_default().to_string_lossy()
                            );
                            write_torrent_file(&file, &bytes, args.force)?;
                            print(&data, &|_| format!("created {file}"));
                        }
                        break;
                    }
                    "FAILED" => return Err(format!("creation failed: {}", job["error"])),
                    "CANCELLED" => return Err("creation was cancelled".to_owned()),
                    _ => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            }
        }
    }
    Ok(())
}

fn field_str(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// Writes the generated .torrent, refusing to clobber an existing file
/// unless `--force` was given.
fn write_torrent_file(file: &str, bytes: &[u8], force: bool) -> Result<(), String> {
    use std::io::Write as _;

    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut out = options.open(file).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            format!("{file} already exists; pass --force to overwrite it")
        } else {
            format!("cannot write {file}: {e}")
        }
    })?;
    out.write_all(bytes)
        .map_err(|e| format!("cannot write {file}: {e}"))
}
