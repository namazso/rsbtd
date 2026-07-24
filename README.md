# rsbtd

A modern BitTorrent daemon.

![Screenshot](https://github.com/user-attachments/assets/d9213e70-8741-4c71-b07e-9dfe3e248fe7)

## Features

- Stable and fully featured: Built on
  [libtorrent-rasterbar](https://libtorrent.org), the same engine as
  qBittorrent and Deluge
- Daemon-native: First class remote control over an efficient and modern
  GraphQL API
- Modern: A stylish WebUI exposing everything you need

## Installing

### Windows

Per-user installers can be found in [Releases](https://github.com/namazso/rsbtd/releases).

Use the tray icon for configuration or opening the web UI in browser.

### Linux (RHEL 10 compatible)

RPMs can be found in [Releases](https://github.com/namazso/rsbtd/releases)

1. Edit `/etc/rsbtd/rsbtd.toml`: set your own `token` and uncomment
   `serve_root = "/usr/share/rsbtd/webui"` so the daemon serves the web UI.
2. The service is sandboxed and can only write to `/var/lib/rsbtd` out of the
   box; allow your download location with a drop-in (`sudo systemctl edit
   rsbtd`):

   ```ini
   [Service]
   ReadWritePaths=/srv/torrents
   ```

   The drop-in only lifts the sandbox — the service runs as the `rsbtd`
   user, so the directory also needs ordinary Unix permissions:

   ```sh
   sudo mkdir -p /srv/torrents
   sudo chown rsbtd:rsbtd /srv/torrents   # or: sudo setfacl -m u:rsbtd:rwx /srv/torrents
   ```

3. `sudo systemctl enable --now rsbtd` and open <http://127.0.0.1:3928/>.

### Linux (container)

Release images are published at `ghcr.io/namazso/rsbtd`. Use them like this:

```sh
podman run -d --name rsbtd --stop-timeout 45 \
    -p 127.0.0.1:3928:3928 \
    -p 6881:6881 -p 6881:6881/udp \
    -e RSBTD_TOKEN=<secret> \
    -v rsbtd-state:/var/lib/rsbtd \
    -v /srv/torrents:/data \
    ghcr.io/namazso/rsbtd:latest
```

- UI served at 3928 by default, you may want to reverse proxy this
- Set `RSBTD_TOKEN` or mount over `/etc/rsbtd/rsbtd.toml` with a full config.
- Make sure you use a long stop grace period (`--stop-timeout 45` above, or
  `podman stop -t 45`)

## Configuration (Linux)

The TOML config file only bootstraps daemon concerns — where to listen, the
token, what to serve:

```toml
state_dir = "/var/lib/rsbtd/state"  # resume data + session state

[api]
listen = "127.0.0.1:3928"           # or a unix socket: "unix:/run/rsbtd/api.sock"
token = "change-me"                 # bearer token; omit to disable auth
serve_root = "/usr/share/rsbtd/webui"  # serve the web UI on / (optional)
#cors = ["https://ui.example.com"]  # origins allowed to call the API (optional)
#graphiql = true                    # GraphiQL IDE on / (instead of serve_root)
#shutdown_grace_secs = 15
```

Torrent engine related settings are available over API or web UI.

## rsbtctl

A oneshot command-line client covering day-to-day operations for scripts and
quick checks; it ships with every build (in the container:
`podman exec rsbtd rsbtctl …`).

```sh
export RSBTCTL_TOKEN=change-me
rsbtctl --url http://127.0.0.1:3928 add --magnet 'magnet:?xt=...' --save-path /data
rsbtctl --url http://127.0.0.1:3928 wait <infohash> --until finished
rsbtctl --unix /run/rsbtd/api.sock list --state seeding --json
rsbtctl --unix /run/rsbtd/api.sock settings set upload_rate_limit=1000000 user_agent=qBittorrent
rsbtctl --unix /run/rsbtd/api.sock settings set 'proxy={"protocol":"socks5","hostname":"10.0.0.1","port":1080,"resolve_hostnames":true,"peer_connections":true,"tracker_connections":true}'
```

## The API

For anything beyond `rsbtctl`: GraphQL on `POST /graphql`, subscriptions via
graphql-ws on the same route (token in the `connection_init` payload),
liveness on `GET /healthz`. Set `graphiql = true` to browse the schema in
GraphiQL.

## Is this vibecoded?

Depends on your definition, but AI was heavily used during development:

- `libtorrent`, the core bittorrent engine, is an external library with a long history
- API layers (rbtorrent, GraphQL) and WebUI UX were human-designed
- Most daemon code is AI-authored and human-reviewed
- WebUI is entirely vibecoded, I'm not a webdev in any capacity
- Code is largely written by Fable 5 and initially reviewed by GPT 5.6 Sol and Kimi K3

## License

MPL-2.0
