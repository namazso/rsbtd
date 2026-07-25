# rbtorrent Examples

This directory contains example programs demonstrating how to use rbtorrent.

## Building Examples

All examples require the `vendored` feature to build libtorrent:

```bash
cargo build --examples --features vendored
```

## Available Examples

### alert_dump

Monitor and print all alerts from a session in real-time.

**Usage:**
```bash
cargo run --example alert_dump --features vendored
```

This example:
- Creates a minimal session with DHT and LSD enabled
- Continuously polls the alert stream
- Prints all alerts with timestamps

**Use cases:**
- Understanding what alerts libtorrent generates
- Debugging session behavior
- Monitoring session activity

### make_torrent

Create a .torrent file from a directory or file.

**Usage:**
```bash
cargo run --example make_torrent --features vendored <input-path> <output.torrent> [tracker-url]
```

**Example:**
```bash
# Create a torrent from a directory
cargo run --example make_torrent --features vendored ./my_files output.torrent

# Create a torrent with a tracker
cargo run --example make_torrent --features vendored ./my_files output.torrent \
  http://tracker.example.com:8080/announce
```

This example:
- Lists all files in the input path
- Calculates piece size automatically
- Hashes all pieces with progress display
- Generates a hybrid BitTorrent v1+v2 torrent file

**Output:**
```
Creating torrent from: ./my_files
Found 3 file(s)
Total size: 1048576 bytes (1.00 MB)
Piece length: 262144 bytes
Number of pieces: 4

Hashing pieces...
Progress: 100% (4/4)

Generating .torrent file...
✓ Successfully created: output.torrent
  Size: 512 bytes
  Pieces: 4
  Piece size: 262144 bytes
  Format: Hybrid (v1 + v2)
```

### cli_download

Simple CLI torrent downloader with progress display.

**Usage:**
```bash
cargo run --example cli_download --features vendored <torrent-file> [save-path]
```

**Example:**
```bash
# Download to current directory
cargo run --example cli_download --features vendored ubuntu-22.04.torrent

# Download to specific directory
cargo run --example cli_download --features vendored ubuntu-22.04.torrent ./downloads
```

This example:
- Loads a .torrent file
- Creates a session with DHT, LSD, UPnP, and NAT-PMP enabled
- Adds the torrent and starts downloading
- Displays real-time progress with download/upload rates, peer counts
- Saves files to the specified directory

**Output:**
```
Loading torrent from ubuntu-22.04.torrent
Starting session...
Adding torrent...
Torrent added successfully
Info hash: InfoHash { ... }

Downloading... Press Ctrl+C to stop.

[45%] ↓ 2048.5 KB/s ↑ 128.3 KB/s | Peers: 12 | Seeds: 3 | State: Downloading
```

## Key Concepts Demonstrated

### Alert Stream Pattern

All examples use the async alert stream pattern:

```rust
let session = Session::new(session_params)?;
let mut alerts = session.alerts();

loop {
    let batch = alerts.next_batch().await?;
    for alert in batch.iter() {
        match alert {
            Alert::ListenSucceeded(a) => { /* ... */ }
            Alert::TorrentFinished(a) => { /* ... */ }
            _ => {}
        }
    }
}
```

**Important:** The alert stream must be polled to drive futures returned by session operations like `add_torrent()`.

### Futures and Alert Correlation

Operations like `session.add_torrent()` return futures that only resolve while the alert stream is being polled:

```rust
let mut alerts = session.alerts();
let add_future = session.add_torrent(&params, std::sync::Arc::new(()));

tokio::pin!(add_future);
let handle = loop {
    tokio::select! {
        result = &mut add_future => break result?,
        batch = alerts.next_batch() => {
            // Process alerts to drive the future
        }
    }
};
```

See `cli_download.rs` for a complete example of this pattern.

### Torrent Creation

Creating a .torrent file involves three steps:

1. **List files** - Scan directory recursively
2. **Create torrent** - Set metadata and properties
3. **Hash pieces** - Compute piece hashes from actual file data

```rust
// 1. List files
let files = list_files(input_path, CreateFlags::empty())?;

// 2. Create torrent with automatic piece size
let mut ct = CreateTorrent::new(&files, 0, CreateFlags::empty())?;
ct.set_creator("My Application")?;
ct.add_tracker("http://tracker.example.com/announce", 0)?;

// 3. Hash pieces; the callback returns whether to keep going
//    (false aborts hashing)
let parent_dir = input_path.parent().unwrap();
set_piece_hashes(&mut ct, parent_dir, Some(|piece_idx| {
    println!("Hashed piece {}", piece_idx);
    true
}))?;

// Generate .torrent file
let torrent_data = ct.generate()?;
std::fs::write("output.torrent", torrent_data)?;
```

See `make_torrent.rs` for a complete example.

## Error Handling

All examples use rbtorrent's `Result` type which wraps `rbtorrent::Error`. For examples that also need to handle other error types (like `std::io::Error`), use `Box<dyn std::error::Error>`:

```rust
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Can use ? with both rbtorrent::Error and std::io::Error
    let params = AddTorrentParams::from_torrent_file(path)?;
    std::fs::write("output", data)?;
    Ok(())
}
```

## More Examples

For more complex usage patterns, see the integration tests in `tests/`:

- `tests/e2e_transfer.rs` - Complete localhost seed/leech transfer
- `tests/alerts.rs` - Alert stream and futures
- `tests/handle.rs` - Torrent handle operations
- `tests/create_torrent.rs` - Comprehensive torrent creation tests

## Documentation

For full API documentation, run:

```bash
cargo doc --features vendored --open
```
