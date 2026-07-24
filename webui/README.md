# rsbtd Web UI

A static single-page web client for the [rsbtd](../README.md) BitTorrent daemon, talking to its
GraphQL API (`POST /graphql` + graphql-ws subscriptions on the same route).

- React 19 + TypeScript + Vite, Tailwind CSS v4, zustand + TanStack Query/Table/Virtual
- Typed GraphQL via graphql-codegen (client preset) from the checked-in `schema.graphql`
- Works on desktop (table UI, context menus) and mobile (list UI, bottom sheets)
- Translation-aware throughout (i18next); English catalog for now

## Development

Prerequisites: Node >= 24 (npm >= 11) and a running rsbtd daemon. There is no dev proxy: the
UI calls the daemon directly, so the daemon's `cors` option must allow the Vite origin — the
dev config below already does.

```sh
# 1. Run a daemon (from the repo root; needs a C++ toolchain + CMake + Boost + OpenSSL,
#    or use a prebuilt rsbtd binary instead of cargo run):
cargo run --release --features vendored -p rsbtd -- --config webui/dev/rsbtd.dev.toml

# 2. Run the UI (from webui/):
npm install
npm run dev                                   # daemon on 127.0.0.1:3928 (default)
RSBTD_URL=http://192.168.1.10:3928 npm run dev  # or any other daemon
```

The dev daemon config (`dev/rsbtd.dev.toml`) has authentication disabled, allows the Vite dev
origin via `cors`, and enables GraphiQL on <http://127.0.0.1:3928/> for exploring the schema.
Uncomment its `token` line to exercise the login flow.

## Scripts

| Script                                  | Purpose                                                                                            |
| --------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `npm run dev`                           | Dev server talking directly to the daemon (`RSBTD_URL` overrides the daemon address)               |
| `npm run build`                         | Typecheck + production build to `dist/`                                                            |
| `npm run codegen`                       | Regenerate `src/gen/` (typed GraphQL + settings catalog); runs automatically before dev/build/test |
| `npm run schema:refresh`                | Re-export `schema.graphql` from the daemon sources and regenerate                                  |
| `npm run test` / `test:watch`           | Vitest unit + component tests                                                                      |
| `npm run lint` / `format` / `typecheck` | ESLint (incl. i18n literal check), Prettier, tsc                                                   |

`schema.graphql` is a checked-in copy of the daemon's SDL (the authoritative contract lives with
the daemon; `tmp/apidoc/` during development). `src/gen/` is generated and gitignored.

## Production

`npm run build` emits a fully static `dist/` with relative asset paths and hash routing — it can
be served from any path by any static file server. Two ways to connect it to a daemon:

- **Let the daemon serve it** (simplest): point the daemon's `serve_root` option at a copy of
  `dist/`. The UI is then same-origin with the API and works with zero configuration.

  ```toml
  # rsbtd.toml
  [api]
  listen = "127.0.0.1:3928"
  token = "change-me"
  serve_root = "/srv/rsbtd-webui"   # contents of webui/dist
  ```

- **Serve it anywhere else** (any static host or CDN): add the UI's origin to the daemon's
  `cors` array, and tell the UI where the daemon is — either on the login screen (Advanced →
  GraphQL endpoint) or with hash params in the URL:

  ```
  https://ui.example.com/#url=https://daemon.example.com:3928&token=change-me
  ```

  `url` takes the daemon address (`/graphql` is appended when no path is given) and is
  persisted; `token` is kept for the browser tab only. Both are stripped from the address bar
  after being read.

Use TLS via a reverse proxy when the traffic leaves the machine; the daemon itself is plain
HTTP. When a `token` is configured in the daemon, the UI asks for it on first load (401) and
sends it as `Authorization: Bearer` / graphql-ws `connection_init` payload.

## Testing

Unit and component tests run with Vitest (+ Testing Library, happy-dom, MSW). The manual
end-to-end checklist against a real daemon lives at [docs/e2e-checklist.md](docs/e2e-checklist.md).
