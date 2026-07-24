# rsbtd

A modern BitTorrent daemon.

## Features

- Stable and fully featured: Built on
  [libtorrent-rasterbar](https://libtorrent.org), the same engine as
  qBittorrent and Deluge
- Daemon-native: First class remote control over an efficient and modern
  GraphQL API
- Modern: A stylish WebUI exposing everything you need

## Installing

Every [release](https://github.com/namazso/rsbtd/releases) carries the Linux
RPMs and the Windows installer. To build from source instead, see
[INSTALL.md](INSTALL.md).

### Container

Release images are published at `ghcr.io/namazso/rsbtd` (`latest` and one tag
per version), multi-arch for x86_64 and aarch64; podman and docker both work:

```sh
podman run -d --name rsbtd --stop-timeout 45 \
    -p 127.0.0.1:3928:3928 \
    -p 6881:6881 -p 6881:6881/udp \
    -e RSBTD_TOKEN=<secret> \
    -v rsbtd-state:/var/lib/rsbtd \
    -v /srv/torrents:/data \
    ghcr.io/namazso/rsbtd:latest
```

The web UI is at <http://127.0.0.1:3928/>. The image ships no default
credentials: set the API bearer token with `RSBTD_TOKEN` (the container
refuses to start without it), or mount your own config at
`/etc/rsbtd/rsbtd.toml` instead (see
[Configuration](#configuration-linux)). Port 3928 is the API and web UI —
keep it on localhost (see [Remote access](#remote-access)). Port 6881 is the
peer listen port (TCP, plus uTP and DHT on UDP; libtorrent's default) —
publish it, and forward it on your router, to accept incoming peer
connections. To use a different peer port, change `listen_interfaces` in the
web UI settings (or via the API) and adjust the `-p` mappings to match. Mount
your download location (`/data` above) and use paths under it as the save
path when adding torrents — `/var/lib/rsbtd` only holds session state and
resume data. Bind-mounted locations must be writable by the container's
`rsbtd` user (fixed UID/GID 947 in the image): `sudo chown 947:947
/srv/torrents`, or with rootless podman the corresponding subordinate ID.
On SELinux hosts add `:Z` to the volume flag (`-v /srv/torrents:/data:Z`)
to relabel. Always stop with a grace period (`--stop-timeout 45` above, or
`podman stop -t 45`) so resume data gets flushed.

### RPMs

Built on Oracle Linux 10 (x86_64 and aarch64) and suitable for any
RHEL-10-compatible distro. Install the three packages from the release page —
`rsbtd` (daemon + systemd service), `rsbtctl`, and `rsbtd-webui` (the web UI
files):

```sh
sudo dnf install ./rsbtd-*.$(uname -m).rpm ./rsbtctl-*.$(uname -m).rpm ./rsbtd-webui-*.noarch.rpm
```

Then:

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

### Windows

Install the MSI from the release page — per-user, no admin rights needed.
rsbtd runs as a tray application that starts with Windows; from the tray menu:

- **Open in browser** opens the web UI, already logged in (a random API token
  is generated at install time).
- **Settings** changes the listen address and state directory; applying
  restarts the daemon in place. An autostart toggle lives in the same menu.

Upgrades keep your token and settings; uninstalling keeps the token and resume
data, so reinstalling brings your torrents back. Choosing a non-localhost
listen address makes Windows show a firewall prompt on the next start.

## Using the web UI

On first load the UI asks for the daemon's token (on Windows this is handled
for you). From there: add torrents by magnet link or .torrent file, pick a
save path (paths are on the machine running the daemon), and manage files,
trackers, and peers per torrent. All engine settings — rate limits, ports,
DHT, proxy, encryption, and every other supported libtorrent option — live in
the UI's settings, apply immediately, and persist in the state directory
across restarts. They are never in the config file.

## Configuration (Linux)

The TOML config file only bootstraps daemon concerns — where to listen, the
token, what to serve:

```toml
state_dir = "/var/lib/rsbtd/state"  # resume data + session state (created 0700)

[api]
listen = "127.0.0.1:3928"           # or a unix socket: "unix:/run/rsbtd/api.sock"
token = "change-me"                 # bearer token; omit to disable auth
serve_root = "/usr/share/rsbtd/webui"  # serve the web UI on / (optional)
#cors = ["https://ui.example.com"]  # origins allowed to call the API (optional)
#graphiql = true                    # GraphiQL IDE on / (instead of serve_root)
#shutdown_grace_secs = 15
```

The RPMs install this at `/etc/rsbtd/rsbtd.toml`. The container image ships
no config file: the entrypoint generates one from `RSBTD_TOKEN` (and optional
`RSBTD_LISTEN`) unless you mount your own at `/etc/rsbtd/rsbtd.toml`, which
then takes precedence. On Windows there is no config file — use the tray
Settings dialog.

## Remote access

The API (and with it the web UI) is plain HTTP: keep it on localhost, or put
a TLS reverse proxy in front when traffic leaves the machine. Alternatively,
host the web UI as static files anywhere (any static host or CDN), add its
origin to the daemon's `cors` list, and point the UI at the daemon on its
login screen or with hash params:

```
https://ui.example.com/#url=https://daemon.example.com:3928&token=…
```

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

Setting names are accepted in snake_case (the libtorrent spelling) or
camelCase (the GraphQL spelling); enum settings take a case-insensitive name
and structured settings take JSON (`proxy=null` disables).

## The API

For anything beyond `rsbtctl`: GraphQL on `POST /graphql`, subscriptions via
graphql-ws on the same route (token in the `connection_init` payload),
liveness on `GET /healthz`. The web UI and `rsbtctl` are ordinary clients of
it and nothing is UI-only — full torrent status, all operations, live event
subscriptions, and every supported libtorrent setting as an explicit,
documented, typed field (~175 scalars plus structured groups like `proxy`,
`i2p`, and `encryption`; a small blacklist of 11 settings that are unsafe or
inert in this build is deliberately not exposed). Set `graphiql = true` to
browse the schema in GraphiQL. In
`applySettings`, omitted fields are unchanged and `null` disables the nullable
groups:

```graphql
mutation {
  applySettings(input: { uploadRateLimit: 1000000, proxy: null }) {
    uploadRateLimit
    proxy { protocol hostname port }
  }
}
```

## Is this vibecoded?

Depends on your definition, but AI was heavily used during development:

- `libtorrent`, the core bittorrent engine, is an external library with a long history
- API layers (rbtorrent, GraphQL) and WebUI UX were human-designed
- Most daemon code is AI-authored and human-reviewed
- WebUI is entirely vibecoded, I'm not a webdev in any capacity
- Code is largely written by Fable 5 and initially reviewed by GPT 5.6 Sol and Kimi K3

## License

MPL-2.0
