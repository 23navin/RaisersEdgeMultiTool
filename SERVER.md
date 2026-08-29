# Running Multitool as a Web Server

The same application ships two ways: a Tauri desktop app and a self-hosted web
server. Both drive the identical engine and the identical React UI, so a
profile that works in one works unchanged in the other.

This document covers the server: how to run it, configure it, connect it to RE
NXT, and what its limits are. For writing profiles see
**[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md)**; for the code architecture see
**[import_tool_reference.md](import_tool_reference.md)**.

---

## Table of Contents

1. [How it fits together](#1-how-it-fits-together)
2. [Quick start](#2-quick-start)
3. [Configuration](#3-configuration)
4. [Connecting to RE NXT](#4-connecting-to-re-nxt)
5. [Running behind a reverse proxy](#5-running-behind-a-reverse-proxy)
6. [The data directory](#6-the-data-directory)
7. [Sessions](#7-sessions)
8. [Concurrency and sizing](#8-concurrency-and-sizing)
9. [HTTP API](#9-http-api)
10. [Security posture](#10-security-posture)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. How it fits together

The Rust code is a Cargo workspace with the engine in the middle and a thin
shell on each side:

```
                    ┌──────────────────────────────┐
                    │        crates/core           │
                    │  profile parsing · DuckDB    │
                    │  validation · SKY API calls  │
                    │  sessions · credentials      │
                    │      (no Tauri, no HTTP)     │
                    └───────┬──────────────┬───────┘
                            │              │
              ┌─────────────┴───┐      ┌───┴──────────────────┐
              │   src-tauri     │      │   crates/server      │
              │ #[tauri::command]│     │ POST /api/<command>  │
              │ native dialogs   │     │ upload · download    │
              │ loopback OAuth   │     │ OAuth callback       │
              └─────────┬────────┘     └──────────┬───────────┘
                        │                         │
                    desktop webview          any browser
                        └────────── src/ ─────────┘
                            one React app,
                        one dist/, one lib/api.ts
```

`crates/core/src/api.rs` holds every operation as a plain function. Each shell
does nothing but build a `Ctx` (where state lives, how to reach RE), call the
function, and translate the result into its transport. The engine has no idea
which one invoked it.

On the frontend, `src/lib/api.ts` is the only file that knows the difference.
At load it checks `'__TAURI_INTERNALS__' in window` and routes every call to
either `invoke()` or `fetch('/api/…')`. The JSON bodies are identical, so the
same `dist/` bundle is served by the Tauri webview and by the web server.

**What differs on the web:**

| Concern | Desktop | Web |
|---|---|---|
| Input files | native dialog yields a local path Rust reads | browser uploads bytes into the session |
| Output files | native Save As dialog + file copy | streamed download with `Content-Disposition` |
| Instruction images | Tauri asset protocol | `GET /api/sessions/{sid}/assets/…` |
| OAuth redirect | loopback listener on `127.0.0.1:13631` | the server's own `/api/oauth/callback` |
| Who is connected to RE | per machine, per user | one connection, shared by everyone |

---

## 2. Quick start

### With Docker (recommended)

```bash
docker compose up --build
# → http://localhost:8080
```

That builds the SPA and the server, then runs with a named volume at `/data`.
With no RE credentials configured it starts in **mock mode**, serving the
fixtures inside each profile bundle — enough to click through every workflow
without touching Blackbaud.

To point it at a real deployment URL:

```bash
PUBLIC_URL=https://multitool.example.org docker compose up -d
```

### From source

```bash
./profiles/build.sh                      # built-ins are embedded at compile time
npm ci && npm run build                  # produces dist/
cargo build --release -p multitool-server

DATA_DIR=./data STATIC_DIR=./dist ./target/release/multitool-server
```

Startup logs the resolved configuration — check this first when something looks
wrong:

```
INFO multitool_server: starting data_dir=/data mode=mock
                       public_url="(derived from Host)" max_runs=4
INFO multitool_server: listening on 0.0.0.0:8080
```

---

## 3. Configuration

Everything is an environment variable. There is no config file, and nothing is
tied to a specific cloud provider — the server needs one writable directory and
a port.

| Variable | Default | Purpose |
|---|---|---|
| `DATA_DIR` | `./data` | **All** persistent state. The one path to mount as a volume. |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address. |
| `STATIC_DIR` | `./dist` | Built frontend to serve. |
| `PUBLIC_URL` | *(derived from `Host`)* | Externally reachable base URL, e.g. `https://multitool.example.org`. Used to build the OAuth `redirect_uri`. **Required behind a proxy.** |
| `MAX_RUNS` | `4` | Concurrent pipeline runs. See [§8](#8-concurrency-and-sizing). |
| `RE_NXT_MOCK` | unset | `1`/`true` forces mock mode even when a connection exists. |
| `RE_CLIENT_ID` | — | Optional headless connection — see [§4](#4-connecting-to-re-nxt). |
| `RE_CLIENT_SECRET` | — | " |
| `RE_SUBSCRIPTION_KEY` | — | " |
| `RE_REFRESH_TOKEN` | — | Seed token, consumed once on first boot. |
| `RUST_LOG` | `info,tower_http=info` | Log filter (`tracing_subscriber` syntax). |

---

## 4. Connecting to RE NXT

### The normal path: through the app

The web build uses the **same Settings → General panel as the desktop app**.
Someone opens Settings, pastes the three values from the Blackbaud developer
portal, and clicks **Connect to Raiser's Edge NXT**:

```
Settings → General
  Application ID      the client ID from your app in the developer portal
  Application secret  the paired secret
  Subscription key    your Primary key from My subscriptions
        │
        ▼  POST /api/connect_re_nxt
  server stashes the credentials against a random state nonce
  and returns Blackbaud's authorization URL
        │
        ▼  browser navigates to oauth2.sky.blackbaud.com
  the user signs in and consents
        │
        ▼  GET /api/oauth/callback?code=…&state=…
  server matches the nonce, exchanges the code for tokens,
  writes DATA_DIR/re_nxt_connection.json, redirects to /?connected=1
```

No terminal, no environment variables, no file copying. The credentials go to
the server over HTTPS and the secret never comes back to the browser.

**One prerequisite you cannot skip:** the redirect URI must be registered on
your application in the Blackbaud developer portal, or sign-in fails with
`redirect_uri_mismatch`. The settings panel displays the exact string to
register for the deployment you're looking at:

```
https://multitool.example.org/api/oauth/callback
```

That's once per deployment, not once per user.

### Who the connection belongs to

The connection is **server-wide**. One person connects; everyone using that
server shares it, and every RE read and write is attributed to that Blackbaud
user in RE's audit log.

Two consequences worth planning around:

- Use a **dedicated RE NXT user** ("Integrations Service"), not a person's
  account. If a staff member leaves and their account is deactivated, an
  integration built on their login dies with it.
- Give that user only the permissions the profiles actually need — query read,
  code table write, whatever the imports touch.

Per-user connections (each person authorizing their own RE account, so RE's own
permissions and audit trail apply individually) need a user identity model the
app does not have yet. The store in `crates/server/src/creds.rs` is written so
it becomes keyed by user without touching the rest of the shell.

### The headless alternative

For deployments where nobody should have to visit the UI — an air-gapped
rollout, automated provisioning — set the credentials in the environment:

```bash
RE_CLIENT_ID=…
RE_CLIENT_SECRET=…
RE_SUBSCRIPTION_KEY=…
RE_REFRESH_TOKEN=…      # one-time seed
```

On first boot, if `DATA_DIR/re_nxt_connection.json` does not exist, the server
writes one from these values with an already-expired access token, forcing a
refresh on first use. From then on the file is authoritative and the rotated
refresh token lives there.

Getting that initial `RE_REFRESH_TOKEN` requires one interactive
authorization, because SKY has no client-credentials grant. Easiest route: run
the desktop app, connect as the service account, and copy `refresh_token` out
of its connection file:

- macOS `~/Library/Application Support/com.navin.tauri-import/re_nxt_connection.json`
- Windows `%APPDATA%\com.navin.tauri-import\re_nxt_connection.json`

The formats are identical, so you can equally copy the **whole file** into
`DATA_DIR/` and skip the env vars entirely.

### One refresh token, one owner

⚠️ Blackbaud rotates the refresh token on every use: refreshing returns a new one
and invalidates the old. The desktop app and the server each persist their own
copy, so **if both run live against the same account they will fight**, and one
will start failing at an unpredictable moment.

Either stop using desktop live mode once the server is up, or register a second
application in the developer portal for desktop use.

### Idle expiry

⚠️ Refresh tokens die after a period of non-use. A server in regular use rotates
and stays alive indefinitely; one that sits quiet over a long holiday will need
re-authorizing. Worth a scheduled health check — call `run_report` on a cheap
report and alert if `mode` comes back `mock` or the call errors.

### Verifying

```bash
curl -s -X POST localhost:8080/api/re_nxt_status \
  -H 'Content-Type: application/json' -d '{}'
# {"connected":true,"environment_name":"…","expires_at":…}
```

`connected` only reflects that a connection file exists. To prove the token
actually works, run a report — that forces a real refresh and a live SKY call
and returns `"mode":"live"`.

---

## 5. Running behind a reverse proxy

Put TLS in front; the server speaks plain HTTP.

```nginx
server {
    listen 443 ssl;
    server_name multitool.example.org;

    location / {
        proxy_pass         http://127.0.0.1:8080;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Forwarded-Proto $scheme;

        # A live SKY query can take minutes; the default 60s gateway timeout
        # will cut long report runs off mid-flight.
        proxy_read_timeout 600s;

        # Vendor files are uploaded whole.
        client_max_body_size 200M;
    }
}
```

**Set `PUBLIC_URL`.** Without it the server derives the OAuth redirect URI from
the `Host` header, which is right for local development but usually wrong
behind a proxy — and any mismatch with the registered URI fails the handshake.

Two proxy settings matter beyond the usual:

- **`proxy_read_timeout`** — RE query polling runs up to ~3 minutes per query
  (`POLL_INTERVAL` × `POLL_MAX_ATTEMPTS` in `crates/core/src/re_calls.rs`), and
  a report runs its queries serially. Reports are still synchronous
  request/response, so a short gateway timeout kills them. See [§8](#8-concurrency-and-sizing).
- **`client_max_body_size`** — the upload endpoint accepts up to 200 MB; the
  proxy must not cap lower.

---

## 6. The data directory

Everything the server persists lives under `DATA_DIR`:

```
DATA_DIR/
├── profiles/                     user .import bundles ("user://<file>" refs)
├── re_nxt_connection.json        the RE NXT connection — SECRET
└── sessions/
    └── s-<token>/                one per profile load
        ├── profile/              extracted bundle (structure.yaml, sql/, fixtures/)
        ├── inputs/               uploaded vendor files
        ├── queries/              re_query results
        ├── codetables/           code table pulls + sync outcomes
        └── runs/<token>/         one dir per transform run → output CSVs
```

**Back up `profiles/` and `re_nxt_connection.json`.** `sessions/` is scratch —
losing it costs nothing but in-flight work.

`re_nxt_connection.json` holds the client secret and refresh token in plaintext.
Restrict the volume's permissions; losing it means re-authorizing, and leaking
it means someone else can act as your service account.

Built-in profiles are **not** here — they are embedded in the binary at compile
time (`BUILTIN_PROFILES` in `crates/core/src/profile.rs`). Adding one means
running `./profiles/build.sh` and rebuilding the image.

---

## 7. Sessions

Loading a profile mints a session; the client gets back an opaque `session_id`
and echoes it on every later call. Files produced by steps cross the wire as
**artifact ids** — session-relative paths like
`runs/18d0…/import_file.csv` — never absolute server paths.

Every id the client sends is re-anchored under its own session directory and
rejected if it contains `..`, an absolute path, or a backslash
(`Workspace::resolve` in `crates/core/src/workspace.rs`). That containment
check is what makes client-supplied ids safe: a caller can only ever name files
inside the session it was given.

Sessions are pure scratch — the server keeps **no in-memory state** for them.
Every request re-reads `structure.yaml` and the SQL from the session directory,
so restarting the server does not disturb sessions already on disk.

Sessions untouched for **24 hours** are deleted, swept on each `load_profile`.
A client returning with a reaped id gets `Session not found — reload the
profile and try again`; reloading mints a new one.

---

## 8. Concurrency and sizing

Each pipeline run creates a full in-process DuckDB instance, and a live RE
query holds a blocking thread while it polls. `MAX_RUNS` (default 4) is a
semaphore in front of every run endpoint; requests beyond it queue rather than
piling up.

Sizing guidance:

- **Memory** is the binding constraint. DuckDB reads input files off disk
  rather than loading them, so transforms are cheap — but result sets returned
  for display are fully materialized as strings in memory. A wide report over
  many rows is the thing that will hurt. Budget a few hundred MB per concurrent
  run plus headroom.
- **CPU** — one core per concurrent run is plenty; the work is I/O-bound far
  more often than not.
- **Disk** — sessions accumulate for up to 24 hours. Size for
  (uploads + outputs) × expected daily runs.

### Long-running reports

⚠️ A report runs its queries **serially**, and each can poll RE for up to ~3
minutes. Reports are still synchronous request/response, so a report with
several slow queries can exceed a proxy's idle timeout and die.

Mitigations today: raise `proxy_read_timeout`, and keep report query counts
modest. The real fix — `POST /api/runs` returning `202` with a job id the
client polls — is designed but not yet built.

---

## 9. HTTP API

All command endpoints are `POST`, take a JSON body, and return JSON. The bodies
are **byte-identical to the Tauri IPC arguments** (camelCase), which is what
lets one frontend serve both targets. Errors return a plain-text body with the
message; the frontend transport throws that string, exactly as a rejected
`invoke()` does on desktop.

### Commands

| Endpoint | Body | Returns |
|---|---|---|
| `POST /api/list_profiles` | `{}` | `ProfileSummary[]` |
| `POST /api/load_profile` | `{zipPath}` | `LoadedProfile` (with `session_id`, `asset_base`) |
| `POST /api/validate_file` | `{filePath, inputLabel, sessionId}` | `ValidationResult` |
| `POST /api/run_profile` | `{filePaths, queryIds, syncIds, sqlFile, sessionId, outputLabels}` | `TransformResult` |
| `POST /api/run_re_query` | `{filePaths, stepLabel, sessionId}` | `QueryStepResult` |
| `POST /api/run_code_table_sync` | `{filePaths, stepLabel, sessionId}` | `SyncResult` |
| `POST /api/run_visualization` | `{filePaths, queryIds, syncIds, stepLabel, sessionId}` | `ResultSet` |
| `POST /api/run_report` | `{sessionId, paramValues}` | `ReportRunResult` |
| `POST /api/run_report_action` | `{sessionId, actionId, paramValues}` | `ActionResult` |
| `POST /api/save_profile` | `{zipPath, files}` | `ProfileMutation` |
| `POST /api/new_profile` | `{}` | `ProfileMutation` |
| `POST /api/duplicate_profile` | `{sourceZipPath}` | `ProfileMutation` |
| `POST /api/delete_profile` | `{zipPath}` | `null` |
| `POST /api/validate_profile` | `{files}` | `ValidationReport` |
| `POST /api/scaffold_missing` | `{files}` | `ProfileFileEntry[]` |
| `POST /api/re_nxt_status` | `{}` | `ConnectionStatus` |
| `POST /api/connect_re_nxt` | `{clientId, clientSecret, subscriptionKey}` | `{authorizeUrl, redirectUri}` |
| `POST /api/disconnect_re_nxt` | `{}` | `null` |

`zipPath` is a **profile ref**, not a path: `builtin://<file>` for an embedded
profile, `user://<file>` for one in `DATA_DIR/profiles/`. `filePaths` maps an
input label to an artifact id from the upload endpoint; `queryIds` / `syncIds`
map upstream labels to artifact ids returned by earlier steps.

### Files and OAuth

| Endpoint | Purpose |
|---|---|
| `POST /api/sessions/{sid}/inputs` | Multipart upload (field name `file`, ≤200 MB) → `{path: "<artifact id>"}` |
| `GET /api/sessions/{sid}/artifacts/{id}?name=out.csv` | Stream an output as a download |
| `GET /api/sessions/{sid}/assets/{rel}` | Serve a file from the session's extracted bundle (instruction images) |
| `GET /api/oauth/callback?code=&state=` | Blackbaud's redirect; redirects to `/?connected=1` or `/?connect_error=…` |

Anything not under `/api/` is served from `STATIC_DIR`, falling back to
`index.html` so client-side routes resolve.

### Worked example

```python
import json, urllib.request
B = "http://localhost:8080"

def post(cmd, body=None):
    r = urllib.request.Request(f"{B}/api/{cmd}",
        data=json.dumps(body or {}).encode(),
        headers={"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(r))

loaded = post("load_profile", {"zipPath": "builtin://re_query_demo.import"})
sid = loaded["session_id"]

# ... upload a file to /api/sessions/{sid}/inputs → file_id ...

q = post("run_re_query", {"filePaths": {"Vendor": file_id},
                          "stepLabel": "FetchFromRE", "sessionId": sid})

tr = post("run_profile", {
    "filePaths": {"Vendor": file_id},
    "queryIds":  {q["query_output"]: q["artifact_id"]},
    "syncIds":   {},
    "sqlFile": "create_import_file.sql",
    "sessionId": sid,
    "outputLabels": ["Update_Records"],
})
print(tr["outputs"][0]["artifact_id"], tr["outputs"][0]["row_count"])
```

---

## 10. Security posture

Be clear-eyed about what this does and does not do today.

**What it protects**

- Client-supplied ids cannot escape their session (lexical containment check on
  every resolve, rejecting `..`, absolute paths, and backslashes).
- Profile refs must be bare `*.import` filenames inside the profiles directory
  — no traversal.
- Bundle extraction rejects archive entries with unsafe paths (zip-slip).
- Paths interpolated into SQL are quote-escaped (`db::sql_path`).
- OAuth uses a 32-byte random CSRF state; a mismatched or expired nonce is
  refused, and nothing is persisted unless the exchange succeeds.
- The RE client secret is only ever held server-side.

**What it does not do yet**

- **No authentication.** Anyone who can reach the port can use the app, run
  imports, and — because the connection endpoints are ungated — connect,
  replace, or disconnect the RE NXT connection. Deploy behind a VPN, an
  authenticating proxy, or SSO. Do not put this on the open internet as-is.
- **Session ids are bearer capabilities.** They are unguessable and scoped, but
  not bound to a browser identity, so anyone holding one can read that
  session's files. Cookie-scoped sessions arrive with login.
- **Profile SQL is arbitrary SQL.** DuckDB runs what the profile author wrote,
  including `COPY … TO` anywhere the process can write. Treat the ability to
  author or upload a profile as equivalent to filesystem access, and restrict
  who can reach Settings → Imports.
- **`re_nxt_connection.json` is plaintext** on the data volume.
- **No rate limiting** beyond the `MAX_RUNS` semaphore.

---

## 11. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `mode=mock` in the startup log, or results are fixtures | No connection and no `RE_*` env vars, or `RE_NXT_MOCK` is set. Connect via Settings → General. |
| `redirect_uri_mismatch` from Blackbaud | The registered URI doesn't byte-match what the server sent. Set `PUBLIC_URL` and register exactly `<PUBLIC_URL>/api/oauth/callback`. |
| `This sign-in link is no longer valid` | The state nonce expired (15 min) or the server restarted mid-handshake. Start the connection again. |
| `Token endpoint returned 400` | Usually a spent or expired refresh token — most often desktop and server sharing one account. See [§4](#one-refresh-token-one-owner). |
| `Session not found — reload the profile` | Session reaped after 24h idle, or `DATA_DIR` changed. Reload the profile. |
| `Artifact '…' not found` | Its session was reaped, or the producing step hasn't run. Re-run the step. |
| Long reports fail at ~60s | Proxy idle timeout. Raise `proxy_read_timeout`. See [§8](#long-running-reports). |
| Uploads fail on large files | Proxy `client_max_body_size` below the server's 200 MB limit. |
| Instruction images don't render | The bundle's `assets/` weren't packed. Re-run `./profiles/build.sh`. |
| Charts render as empty dashed boxes | Only `table` is implemented in `VIZ_REGISTRY`; the chart types are placeholders. Not server-specific. |

Turn up logging with `RUST_LOG=debug,tower_http=debug` to see each request and
the full error chain.
