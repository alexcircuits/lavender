# Getting Started

Commands below run from **`products/lavender`**, unless a block states otherwise.
In a standalone Lavender clone, that means the repository root. The Rust toolchain
is pinned by [`rust-toolchain.toml`](../../rust-toolchain.toml); the web package
owns its install and lockfile independently of the monorepo.

## Prerequisites and install

Install Rust with rustup, a native compiler toolchain (Xcode command-line tools on
macOS), Node.js 22.12 or newer, and pnpm 12.11.2. Rustup selects Rust 1.99.0 when
Cargo runs in this directory. Check what your shell actually uses:

```bash
cargo --version
rustc --version
node --version
pnpm --version
cargo build --locked -p lavender-server
pnpm --dir apps/web install --frozen-lockfile
```

If the pnpm command is an older shim, this verified alternative uses the exact
package manager without changing the global installation:

```bash
npm exec --yes --package=pnpm@12.11.2 -- pnpm --dir apps/web install --frozen-lockfile
```

Node 22.13+ is needed for pnpm 12's npm installer. The
[official standalone installation instructions](https://pnpm.io/installation)
are another option. Keep the monorepo's own pnpm pin; changing it is outside a
Lavender feature. No database or provider key is needed for the three existing feeds.

## Run with hot reload

In one terminal, from Lavender's root:

```bash
cargo run --locked -p lavender-server
```

In a second terminal, from the same root:

```bash
pnpm --dir apps/web run dev
```

Open `http://localhost:5410`. Vite proxies `/api` and the live WebSocket to Rust on
`127.0.0.1:7410`. Keep Vite's port fixed because the server checks browser origins.

Open `http://localhost:5410/docs/` for the browser developer guides. The docs use
the same Vite process and can be read with only the web server running.

In the **NoirSystems monorepo root**, the same two processes can be started with:

```bash
pnpm exec nx run lavender:dev
```

Nx manages both commands and launches the web package with its pinned pnpm 12
through npm, independently of the monorepo's pnpm 10. The first run needs registry
access to populate npm's package cache. Stop both processes with Ctrl+C. The existing
`lavender-web:dev` target starts Vite alone.

### Predictable fixture data

Replace the backend command with:

```bash
LAVENDER__SERVER__FIXTURES_DIR=apps/web/e2e/fixtures cargo run --locked -p lavender-server
```

Each feed says "Recorded fixture data, not live." The same parsers and API run,
but providers read local documents. Globe imagery may still contact catalog-listed
tile hosts; fixture mode only replaces feed providers. In the monorepo, prefix
`pnpm exec nx run lavender:dev` with the same environment assignment.

## Build and serve the whole application

```bash
pnpm --dir apps/web run build
cargo build --locked -p lavender-server
cargo run --locked -p lavender-server
```

Open `http://127.0.0.1:7410`. Rust serves `apps/web/dist` as well as the API. Node
and pnpm are build/development tools; no Node server is needed to serve this app.
Without `dist/index.html`, Rust starts successfully in API-only mode and logs why.
The built developer documentation is at `http://127.0.0.1:7410/docs/`.

## Tests, formatting and lint

```bash
cargo test --locked
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo deny --locked check
./scripts/check-contracts.sh
pnpm --dir apps/web run typecheck
pnpm --dir apps/web run test
pnpm --dir apps/web run lint
pnpm --dir apps/web run build
```

Plain Cargo commands use default workspace members and skip desktop. Install
`cargo-deny` if missing (`cargo install cargo-deny --locked`). For the complete
backend/web check, run `./scripts/check.sh`; it includes all checks above and a
server build. From the monorepo root, use `pnpm exec nx run lavender:check`.
If using the exact npm-provided pnpm, run
`npm exec --yes --package=pnpm@12.11.2 -- bash scripts/check.sh` from Lavender's root.

Apply formatting with `cargo fmt --all` and `pnpm --dir apps/web run format`.
E2E and optional desktop checks are in [Testing](testing.md).

## Configuration and optional credentials

The server reads defaults, an optional TOML file, then `LAVENDER__SECTION__KEY`
environment overrides. It **does not automatically load `.env`**. This checked-in
[development example](../../config/development.example.toml) contains no keys:

```bash
cargo run --locked -p lavender-server -- --config config/development.example.toml
```

`LAVENDER_CONFIG` also selects a file. To change the listen address, set
`LAVENDER__SERVER__ADDR`; to change logging, set `RUST_LOG` or
`LAVENDER__SERVER__LOG_FORMAT=json`. Unknown fields fail validation at startup.

[`SecretKey::env_var`](../../crates/lavender-core/src/secrets.rs) recognizes
`LAVENDER_FIRMS_MAP_KEY`, `LAVENDER_AISSTREAM_API_KEY`, `LAVENDER_OPENSKY_CLIENT_ID`
and `LAVENDER_OPENSKY_CLIENT_SECRET`, or each name with `_FILE` appended for a
secret file. These sources are **not implemented yet**; setting a key does not add
a feed. Existing USGS, adsb.lol and CelesTrak feeds need no credential. Supply keys
to the Rust process through your local secret tooling; never add them to Vite
variables, committed TOML, workspace preferences, screenshots or test fixtures.

Default licence mode is `evaluation`. `commercial` enforces the registry's
restrictions and may disable sources. An acknowledgement represents an actual
provider agreement; it is not a development workaround. See the
[licensing inventory](../audit/data-sources-and-licensing.md).

## Common setup problems

| Symptom | What to check |
|---|---|
| `pnpm` fails looking for `bin/pnpm.cjs` | An old Corepack shim cannot launch this pnpm version; use the exact npm command above or standalone installation. |
| Rust version or edition error | Run from Lavender's root with rustup on PATH; check `rustc --version` against the toolchain file. |
| `Address already in use` | Stop the process using 7410, 5410 or E2E's 7412; changing only the Rust port breaks Vite's proxy. |
| API works but `/` is missing | Build `apps/web/dist` or use Vite on 5410; API-only startup is expected before a web build. |
| Browser gets 403 | Use the documented localhost URL; inspect Host/Origin and the server's allowed hosts. |
| Aircraft is partial or degraded | Zoom in; large views use military-only fallback. Live adsb.lol may throttle; use fixtures for tests. |
| Satellites disappear in commercial mode | Check source availability and licence status in the catalog, rather than changing the license gate. |
| Playwright browser missing | Install Chromium or choose installed Chrome as described in Testing. |
| Desktop system-library error | Desktop is optional. Use default Cargo members for web work; read Web and Desktop for native prerequisites. |

First dependency builds may take several minutes. Check command output before
changing lockfiles or upgrading dependencies to fix a local environment problem.
