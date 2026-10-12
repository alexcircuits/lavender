# Lavender technology research — October 2026

- **Date:** 2026-10-11
- **Scope:** (A) working practices for Claude Opus 5.5, (B) Tauri 2, (C) WebView engines and WebGL, (D) CesiumJS, (E) Rust crates, (F) recommendations.
- **Method:**
  - Version numbers come from the registry APIs, queried on 2026-10-11 (crates.io `/api/v1/crates/<name>` and `registry.npmjs.org/<name>`). Each is cited as the package page.
  - Behaviour claims cite official docs, upstream source, changelogs or issue threads.
  - Third-party sources are labelled as such.
  - Anything I could not confirm is marked **UNVERIFIED**.
- **Direct tests:** the CORS preflight results in C.7 come from `curl` runs from this machine on 2026-10-11.

---

## A. Working with Claude Opus 5.5 and other current Claude models

Opus 5.5 is one of the current models the official prompting guide covers. That guide splits its advice into model-specific guidance and techniques that apply to all models ([prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)). Opus 5.5 has a dedicated page ([Prompting Claude Opus 5.5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5-5)).

### Actionable practices (25)

1. **Use effort as the main control.** On Opus 5.5, `medium` is the default, thinking is always on, and `thinking: {"type": "disabled"}` returns a 400 error. Run an effort sweep instead of carrying settings over from older models ([effort](https://platform.claude.com/docs/en/build-with-claude/effort#recommended-effort-levels-for-claude-opus-5-5)).
2. **Save `xhigh` and `max` for measured gains.** `xhigh` is described as "Long-running agentic and coding tasks (over 30 minutes)". The Opus 5.5 guide says to "Reserve `xhigh` and `max` for work where you've measured a quality gain" ([effort](https://platform.claude.com/docs/en/build-with-claude/effort); [Opus 5.5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5-5#calibrate-effort)).
3. **To get less thinking, lower effort rather than prompting for it.** Also remove "think carefully" lines, which on Opus 5.5 mostly add latency ([Opus 5.5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5-5#thinking-instructions-in-chat-system-prompts)).
4. **Be explicit, and explain why.** The guide's golden rule is to write a prompt that a colleague with minimal context could follow, and to give the reason behind each constraint ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#be-clear-and-direct)).
5. **Ask for action, not suggestions.** "Change this function…" gets edits, while "Can you suggest some changes…" gets suggestions ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#tool-usage)).
6. **Give the complete spec up front, then let it run.** Opus 5 is documented to perform best that way ([Opus 5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5#capability-improvements)).
7. **Constrain scope explicitly.** Use the documented line "Deliver what was asked, at the scope intended…" so the model neither quietly widens nor narrows the task ([Opus 5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5#task-scope-and-over-verification)).
8. **Don't tell the model to "double-check"; give it a check it can run.** Opus 5 verifies on its own, and explicit verification instructions cause over-verification ([Opus 5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5#self-correction)). What closes the loop is a test, build, linter or screenshot that returns pass or fail ([Claude Code best practices](https://code.claude.com/docs/en/best-practices#give-claude-a-way-to-verify-its-work)).
9. **Require evidence, not assertions.** Ask for the command that was run and what it returned, the test output, or a screenshot ([Claude Code best practices](https://code.claude.com/docs/en/best-practices#give-claude-a-way-to-verify-its-work)).
10. **Treat a turn that ends in text as a report, not proof the task is done.** Keep a checklist file that the model updates. Re-prompt with the open items, stop after two or three automatic continuations, and name the early-stop patterns you don't want ([Opus 5.5, unattended runs](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5-5#unattended-agentic-runs)).
11. **Use the first context window for scaffolding.** Have it write the tests and an `init.sh`, then iterate through a to-do list in later windows ([best practices, multi-window](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#workflows-across-multiple-context-windows)).
12. **Treat tests as the spec, and keep them in structured form.**
    - Track tests in `tests.json`, with the instruction "It is unacceptable to remove or edit tests…" ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#workflows-across-multiple-context-windows)).
    - Anthropic's harness kept the feature list as JSON with a `passes` field. The authors found that the model is less likely to inappropriately overwrite JSON than Markdown ([Effective harnesses, 2025-11-26](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)).
13. **Use free-form progress notes plus git as memory.** Start each session with `pwd`, read the progress file and `git log`, and run a basic end-to-end check before starting new work ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#state-management-best-practices); [harnesses](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)).
14. **Do one feature per session and end in a mergeable state.** The harness article defines "clean state" as no major bugs, well organised and documented ([harnesses](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)).
15. **Tell the model when context will be compacted.** Otherwise it may "naturally try to wrap up work as it approaches the context limit." For a hard reset, a fresh window plus the filesystem state can beat compaction ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#context-awareness-and-multiwindow-workflows)).
16. **Treat context as the scarce resource.**
    - Run `/clear` between unrelated tasks.
    - After two failed corrections, restart with a better prompt.
    - Scope investigations tightly, or hand them to subagents ([Claude Code best practices](https://code.claude.com/docs/en/best-practices#avoid-common-failure-patterns)).
    - Anthropic's context-engineering guidance: keep the smallest high-signal context, retrieve just in time, and take structured notes ([Effective context engineering, 2025-09-29](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)).
17. **Delegate only independent, parallel, or context-isolating work.** Don't use subagents for small tasks or to verify your own work. Opus 5 delegates readily; cap it with a prompt, or with the `CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH` and `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS` environment variables ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#subagent-orchestration); [Opus 5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5#controlling-subagent-spawning)).
18. **Before calling work done, have a fresh-context subagent review the diff against the plan.** Tell the reviewer to report only gaps that affect correctness or the stated requirements. Chasing every finding leads to over-engineering ([Claude Code best practices](https://code.claude.com/docs/en/best-practices#add-an-adversarial-review-step)).
19. **Use a separate evaluator, not self-grading.** Agents tend to praise their own output, and a standalone evaluator is easier to tune to be skeptical. Turn taste into gradable criteria: design quality, originality, craft and functionality. Calibrate the evaluator with examples ([Harness design for long-running apps, 2026-03-24](https://www.anthropic.com/engineering/harness-design-long-running-apps)).
20. **Re-check harness scaffolding as models improve.** For example, context resets were dropped once Opus 4.5 largely removed "context anxiety" ([harness design](https://www.anthropic.com/engineering/harness-design-long-running-apps)).
21. **Prevent over-engineering and test-gaming with explicit text.** The guide has documented snippets for both: "Only make changes that are directly requested or clearly necessary…" and "Tests are there to verify correctness, not to define the solution" ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#overeagerness)).
22. **Investigate before answering.** "Never speculate about code you have not opened" ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#minimizing-hallucinations-in-agentic-coding)).
23. **For frontend design, name the specific patterns to avoid.** On Opus 5.5, a generic "avoid a generic AI look" mostly swaps one default for another. List concrete bans, check which styles the result used, and extend the list ([Opus 5.5, frontend defaults](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5-5#frontend-design-defaults)).
24. **Avoid "AI slop" with deliberate design choices** ([best practices, frontend](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#frontend-design); [claude.com blog, 2025-11-12](https://www.claude.com/blog/improving-frontend-design-through-skills)):
    - Pick distinctive typography instead of Inter, Roboto or Arial.
    - Commit to a palette through CSS variables, and avoid purple-on-white gradients.
    - Spend motion on one orchestrated moment.
    - Use layered, atmospheric backgrounds.
25. **Give the model a way to see the UI.** Use browser automation or computer-use tools, and test "as a human user would". Note one blind spot: Puppeteer could not see native `alert()` modals ([best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#workflows-across-multiple-context-windows); [harnesses](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)).

**For Lavender:**
- Keep `docs/progress.md` (free-form notes) and a JSON test/feature ledger per milestone.
- Provide an `init.sh` that starts the Axum server and the Vite dev server and runs the smoke suite.
- Make the definition of done a screenshot of the globe plus test output, not prose.

---

## B. Tauri 2.x

### B.1 Versions and release status (as of 2026-10-11)

| Item | Version / date | Source |
|---|---|---|
| `tauri` (Rust) | **2.12.3**, 2026-10-10 | [crates.io/crates/tauri](https://crates.io/crates/tauri) |
| `tauri-build` | 2.7.2 | [crates.io](https://crates.io/crates/tauri-build) |
| `@tauri-apps/api` | 2.12.3 (`latest`); `next` = 3.0.0-alpha.2 | [npm](https://www.npmjs.com/package/@tauri-apps/api) |
| `@tauri-apps/cli` | 2.12.2 | [npm](https://www.npmjs.com/package/@tauri-apps/cli) |
| Tauri 2.12 announcement | 2026-09-26. The post calls it "the biggest update so far in the 2.x releases". It raises the MSRV to **1.90**, drops Windows 7, and adds an `on_permission_request` handler | [blog](https://v2.tauri.app/blog/tauri-2.12/) |
| Tauri 3 | **3.0.0-alpha.4** (2026-10-01). The first alpha shipped 2026-09-13 with an MSRV of 1.95 and pluggable runtimes, `tauri-runtime-wry` or **`tauri-runtime-cef`** (3.0.0-alpha.5). It is not production-ready | [crates.io versions](https://crates.io/crates/tauri/versions); [v3.0.0-alpha.0 release notes](https://github.com/tauri-apps/tauri/releases/tag/tauri-v3.0.0-alpha.0); [tauri-runtime-cef](https://crates.io/crates/tauri-runtime-cef) |
| Plugins | store 2.5.0, sql 2.5.0, stronghold 2.4.0, http 2.8.2, updater 2.13.3, deep-link 2.6.1, single-instance 2.5.3, window-state 2.5.0, shell 2.5.0 | crates.io: [store](https://crates.io/crates/tauri-plugin-store), [sql](https://crates.io/crates/tauri-plugin-sql), [stronghold](https://crates.io/crates/tauri-plugin-stronghold), [http](https://crates.io/crates/tauri-plugin-http), [updater](https://crates.io/crates/tauri-plugin-updater), [deep-link](https://crates.io/crates/tauri-plugin-deep-link), [single-instance](https://crates.io/crates/tauri-plugin-single-instance), [window-state](https://crates.io/crates/tauri-plugin-window-state), [shell](https://crates.io/crates/tauri-plugin-shell) |

### B.2 Commands, state and errors

**Commands** ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)):
- Annotate with `#[tauri::command]` and register every command in a single `tauri::generate_handler!`. If you call `invoke_handler` more than once, "Only the last call will be used."
- Commands defined in `lib.rs` cannot be `pub`. Commands in a separate module should be `pub`.

**Sync vs async:**
- Sync commands run on the main thread unless they are marked `#[tauri::command(async)]`. Async commands run on the async runtime.
- Async commands cannot take borrowed arguments such as `&str` or `State<'_, T>` unless they return a `Result` ([Calling Rust](https://v2.tauri.app/develop/calling-rust/); [state management](https://v2.tauri.app/develop/state-management/)).

**Arguments:**
- Arguments arrive as JSON with camelCase keys. `rename_all = "snake_case"` changes that.
- Argument types must implement `Deserialize`, and return types must implement `Serialize` ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)).

**Errors:**
- Returning `Result<T, E>` rejects the JS promise on `Err`, so `E` must be serialisable.
- The documented pattern is a `thiserror` enum with a manual `Serialize` implementation ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)).

**State:**
- Register with `.manage(...)` and inject `State<'_, T>`. You don't need `Arc`, because Tauri wraps managed state.
- Use `std::sync::Mutex` by default. Use Tokio's mutex only when a guard must be held across an `.await`.
- A type mismatch such as `State<T>` vs `State<Mutex<T>>` is not caught at compile time. At runtime, `invoke` rejects with "state not managed", and `Manager::state()` panics ([state management](https://v2.tauri.app/develop/state-management/)).

**Binary results:** large JSON results "can slow down your application". Return `tauri::ipc::Response` for binary payloads, and use `tauri::ipc::Request` to read raw request bodies ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)).

**Process model:** keep business logic and secrets in the core process: "never handle secrets in the Frontend", and "defer as much business logic as possible to the Core process" ([process model](https://v2.tauri.app/concept/process-model/)). This matches Lavender's ADR 0002.

Illustrative sketch, not compiled, following the documented patterns:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CmdError {
    #[error(transparent)]
    Core(#[from] lavender_core::Error),
}
impl serde::Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string()) // or a { code, message } struct
    }
}

#[tauri::command]
pub async fn snapshot(
    core: tauri::State<'_, lavender_core::LavenderCore>,
    query: FeedQuery,
) -> Result<FeedSnapshot, CmdError> {
    Ok(core.snapshot(query).await?)
}

#[tauri::command]
pub async fn subscribe(
    core: tauri::State<'_, lavender_core::LavenderCore>,
    query: FeedQuery,
    on_update: tauri::ipc::Channel<FeedSnapshot>,
) -> Result<(), CmdError> {
    // spawn a task that forwards core.subscribe(query) items via on_update.send(..)
    // and stops when send() fails or a cancel command fires
    Ok(())
}
```

### B.3 Channels vs events

| | `tauri::ipc::Channel<T>` | Events (`emit` / `listen`) |
|---|---|---|
| Purpose | "the recommended mechanism for streaming data" ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)) | Small notifications, broadcast to windows |
| Guarantees | "designed to be fast and deliver ordered data". Used internally for download progress, child-process output and WebSocket messages ([Calling the frontend](https://v2.tauri.app/develop/calling-frontend/)) | "not designed for low latency or high throughput situations"; JSON only; async listeners can process events out of order ([Calling the frontend](https://v2.tauri.app/develop/calling-frontend/)) |
| Scope | Bound to one `invoke` call. It lives "until the Rust `Channel` is dropped" ([JS core API](https://v2.tauri.app/reference/javascript/api/namespacecore/)) | Global, or per webview through `emit_to` / `emit_filter`. Webview-specific events do not trigger global listeners ([Calling the frontend](https://v2.tauri.app/develop/calling-frontend/)) |
| JS side | `const ch = new Channel<T>(); ch.onmessage = …; await invoke('cmd', { onEvent: ch })` | `listen<T>()` returns an unlisten promise. Don't call `unlisten()` before that promise resolves ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)) |

Use Channels for Lavender's `subscribe(FeedQuery)` streams and events only for app-level signals, such as "settings changed".

### B.4 Capabilities and permissions

- **Location and loading:** capability files (JSON or TOML) live in `src-tauri/capabilities/`. "All capabilities inside the `capabilities` directory are automatically enabled by default", unless `app.security.capabilities` lists them explicitly ([capabilities](https://v2.tauri.app/security/capabilities/)).
- **Fields:** `identifier`, `description`, `windows`, `permissions`, an optional `platforms`, and `remote.urls` for remote origins. A window that appears in several capabilities gets the merged permissions. Scoping uses window **labels**, not titles ([capabilities](https://v2.tauri.app/security/capabilities/)).
- **What capabilities don't protect against:**
  - Malicious Rust code, overly broad scopes, and wrong scope checks inside commands.
  - WebView vulnerabilities and supply-chain attacks ([capabilities](https://v2.tauri.app/security/capabilities/)).
  - On Linux and Android, Tauri cannot tell an `<iframe>` from the window when granting remote access ([capabilities](https://v2.tauri.app/security/capabilities/)).
- **Permission identifiers:** `<name>:default` and `<name>:<command>`. App permission files go in `src-tauri/permissions/*.toml` ([permissions](https://v2.tauri.app/security/permissions/)).
- **App commands:** `tauri_build::AppManifest::commands` auto-generates `allow-$command` and `deny-$command` permissions for app commands ([docs.rs](https://docs.rs/tauri-build/latest/tauri_build/struct.AppManifest.html)). Whether app commands are callable without any capability entry when no `AppManifest` is configured is **UNVERIFIED** from the docs I read. Lavender should configure `AppManifest::commands` and grant each command explicitly. The v3 alpha adds implicit app-wide `allow-*` and `deny-*` ([v3 alpha.0 notes](https://github.com/tauri-apps/tauri/releases/tag/tauri-v3.0.0-alpha.0)).

### B.5 Content Security Policy

- **Enablement:** "The CSP protection is only enabled if set on the Tauri configuration file". The template ships `"csp": null` ([CSP](https://v2.tauri.app/security/csp/); [create-tauri-app template](https://github.com/tauri-apps/create-tauri-app/blob/dev/templates/_base_/src-tauri/%25(v2)%25tauri.conf.json.lte)).
- **Nonces and hashes:** at compile time Tauri hashes local scripts and references styles and external scripts by nonce. It appends those sources to `script-src` / `style-src` ([CSP](https://v2.tauri.app/security/csp/)).
  - Upstream source shows the nonce or hash is added **only when the bundled HTML actually contains nonce tokens or inline hashes** ([`manager/mod.rs`](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/manager/mod.rs)).
- **WebAssembly:** projects using WebAssembly must add `'wasm-unsafe-eval'` to `script-src` ([CSP](https://v2.tauri.app/security/csp/)).
- **IPC and the asset protocol:** the documented example includes `connect-src ipc: http://ipc.localhost` and `img-src … asset: http://asset.localhost blob: data:` ([CSP](https://v2.tauri.app/security/csp/)).
- **Config keys** ([config reference](https://v2.tauri.app/reference/config/)):
  - `app.security.csp`: injected into all HTML files in production. It is also used in development when `devCsp` is unset.
  - `devCsp`: the development-only policy.
  - `dangerousDisableAssetCspModification`: `true`, or a list of directives "that Tauri cannot inject".
  - `freezePrototype`: runs `Object.freeze(Object.prototype)`. Libraries that extend built-in prototypes may break; whether CesiumJS is affected is **UNVERIFIED**.
  - `headers`: extra response headers.
- **Gotcha that matters for Cesium:** under CSP Level 3, if a directive contains any nonce or hash, `'unsafe-inline'` is ignored for that directive ([W3C CSP3 §6.7.3.2](https://www.w3.org/TR/CSP3/#allow-all-inline)).
  - CesiumJS's `CreditDisplay` injects a `<style>` element at runtime ([source](https://github.com/CesiumGS/cesium/blob/main/packages/engine/Source/Scene/CreditDisplay.js)).
  - So keep `index.html` free of inline `<style>`/`<script>`, or set `dangerousDisableAssetCspModification: ["style-src"]`, and keep `style-src 'self' 'unsafe-inline'`. This conclusion is my inference from the three sources above; confirm it in a smoke test.

### B.6 Custom URI schemes, asset protocol and origins

- **Custom schemes:** `Builder::register_uri_scheme_protocol` and `register_asynchronous_uri_scheme_protocol` register a scheme for all webviews. The async variant hands you a `UriSchemeResponder`, so you can respond from another thread ([docs.rs Builder 2.12.3](https://docs.rs/tauri/latest/tauri/struct.Builder.html)).
- **Origins differ by platform:** `<scheme>://localhost/<path>` on macOS, iOS and Linux; `http://<scheme>.localhost/<path>` on Windows and Android. Servers that enforce CORS must allow those origins ([docs.rs Builder](https://docs.rs/tauri/latest/tauri/struct.Builder.html)). The app itself therefore loads from `tauri://localhost` on macOS and Linux and from `http://tauri.localhost` on Windows.
- **`useHttpsScheme`** (Windows and Android, default `false`):
  - Switches to `https://<scheme>.localhost`, which then blocks mixed-content fetches to `http://`.
  - Changing it between releases moves IndexedDB, cookies and localStorage, so existing data becomes inaccessible ([config reference](https://v2.tauri.app/reference/config/)).
- **Asset protocol:** `app.security.assetProtocol` defaults to `{enable:false, scope:[]}` ([config reference](https://v2.tauri.app/reference/config/)). `convertFileSrc(path)` turns a filesystem path into a URL, which needs the scope plus CSP entries `asset: http://asset.localhost` ([`core.ts` source](https://github.com/tauri-apps/tauri/blob/dev/packages/api/src/core.ts)).
- **Option for Lavender:** an async custom scheme such as `lavender://` can serve cached tiles or large binary layers to Cesium without a localhost socket. Tauri's protocol handlers use the `http` crate types since 2.0.0-alpha.12 ([release notes](https://v2.tauri.app/release/tauri/v2.0.0-alpha.12)), which in principle lets an Axum `Router` be driven via `tower::ServiceExt::oneshot`. No official example of that exists (**UNVERIFIED** as a documented pattern).
- **`tauri-plugin-localhost`:** serves assets from a real localhost server, but "brings considerable security risks" ([localhost plugin](https://v2.tauri.app/plugin/localhost/)). ADR 0002 already rejects a localhost listener.

### B.7 Local storage and secrets

| Option | What it is | Fit for Lavender |
|---|---|---|
| `tauri-plugin-store` 2.5.0 | Persistent key-value **JSON file**. Values are `serde_json::Value`, saving is debounced, and relative paths resolve under `BaseDirectory::AppData` ([store docs](https://v2.tauri.app/plugin/store/); [store.rs](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/store/src/store.rs)). Not described as encrypted. | UI preferences only, never secrets. Or simply let the core own a settings file (ADR 0002 `SettingsStore`). |
| `tauri-plugin-sql` 2.5.0 | sqlx-based (sqlx **0.8**) SQLite, MySQL or Postgres. The SQLite path is relative to `AppConfig`, migrations run in transactions, and **queries are issued from the frontend**, gated by `sql:allow-execute` ([sql docs](https://v2.tauri.app/plugin/sql/); [Cargo.toml](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/sql/Cargo.toml)) | Not recommended. It puts SQL in the WebView and bypasses the core. Use rusqlite or sqlx inside `lavender-core` instead. |
| `tauri-plugin-stronghold` 2.4.0 | IOTA Stronghold vault. Needs a 32-byte password hash (Argon2 helper) ([stronghold docs](https://v2.tauri.app/plugin/stronghold/)). The upstream `iota_stronghold` crate's last release is 2.1.0 on 2024-05-13 ([crates.io](https://crates.io/crates/iota_stronghold)). Plugin 3.0.0-alpha builds exist for Tauri 3 ([changelog](https://github.com/tauri-apps/plugins-workspace/blob/v3/plugins/stronghold/CHANGELOG.md)). I found **no official deprecation notice** ("deprecated in v3" is **UNVERIFIED**). | Avoid. It needs a user password or a stored key, and a dormant upstream. |
| `keyring` crate 4.2.0 (Rust, in a command) | Native secure stores on macOS, Windows and *nix through the default `v1` API. The library parts moved to `keyring-core` 1.0.0, with stores split into separate crates ([README](https://github.com/open-source-cooperative/keyring-rs); [crates.io](https://crates.io/crates/keyring)) | **Recommended.** Real-world precedent: GeoLibre (a Tauri GIS app) moved tokens, including a Cesium ion token, into the OS keychain via `keyring` (pinned `=3.6.3`) ([PR #2717](https://github.com/opengeos/GeoLibre/pull/2717)). |
| `tauri-plugin-keyring` (community) 0.1.0 | Wrapper over keyring, last release 2024-12-23 ([crates.io](https://crates.io/crates/tauri-plugin-keyring); [repo](https://github.com/HuakunShen/tauri-plugin-keyring)) | Unnecessary. Call `keyring` from Rust instead. |
| Official `secure-storage` plugin | Exists only on a `plugin/secure-storage` branch of plugins-workspace ([branch README](https://github.com/tauri-apps/plugins-workspace/blob/plugin/secure-storage/plugins/secure-storage/README.md)). Release status is **UNVERIFIED** (not on crates.io as of this date). | Watch it. |

### B.8 HTTP: `tauri-plugin-http` vs Rust-side `reqwest`

- **What the plugin is:** `tauri-plugin-http` re-exports `reqwest` and offers a JS `fetch` that "tries to be as close and compliant to the" Fetch API ([http plugin](https://v2.tauri.app/plugin/http-client/)).
  - The default permission enables fetch but **no origins**. You allow and deny URL patterns under `http:default`, and forbidden request headers are dropped unless you enable the `unsafe-headers` feature ([http plugin](https://v2.tauri.app/plugin/http-client/)).
  - Plugin 2.8.2 still depends on **reqwest 0.12** ([Cargo.toml](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/http/Cargo.toml)), while current reqwest is 0.13.5 ([crates.io](https://crates.io/crates/reqwest)).
- **Recommendation:** do provider I/O in `lavender-core` with reqwest 0.13, which brings the core's allowlists, timeouts, size limits and secrets that never touch JS. Don't grant the WebView `http:` permissions.
- **Exception:** Cesium tile and imagery requests from the WebView go through the browser's own `fetch`/XHR, subject to CSP `connect-src` and to CORS (see C.7).

### B.9 App data directories

All of these are on `app.path()` ([PathResolver docs](https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html)):
- `app_data_dir()` resolves to `data_dir/${bundle_identifier}`:
  - Linux: `$XDG_DATA_HOME`, falling back to `~/.local/share`.
  - macOS: `~/Library/Application Support`.
  - Windows: `{FOLDERID_RoamingAppData}`.
- `app_local_data_dir()` is the WebView's default data dir on Windows and Linux.
- `app_config_dir()`, `app_cache_dir()` and `app_log_dir()` work the same way.
- All of them can be overridden by `app > appDirectoriesOverride`.

Changing the bundle `identifier` moves every one of these directories.

### B.10 Sidecars, and why to avoid a Node sidecar

- **How sidecars work:**
  - Declare binaries in `bundle.externalBin`, with one copy per target, named `name-$TARGET_TRIPLE`.
  - Run them with `app.shell().sidecar(name)` or `Command.sidecar(name)`.
  - Grant `shell:allow-execute` / `shell:allow-spawn` with `"sidecar": true` and argument validators ([sidecar](https://v2.tauri.app/develop/sidecar/)).
- **The Node route:** the official Node guide packages the app with `@yao-pkg/pkg`. It applies to desktop only. It notes that the embedded runtime is "usually larger than a pkg packaged application", and that long-running sidecars need localhost servers, stdin/stdout or sockets, "each with its own tradeoffs and security concerns" ([Node sidecar](https://v2.tauri.app/learn/sidecar-nodejs/)).
- **Why Lavender should avoid it:** a Node sidecar would duplicate the Rust core, add per-target binaries and an IPC transport, and reintroduce exactly the second backend that ADR 0002 avoids. That is a design judgment, not a sourced fact.

### B.11 Updater and signing

- **Signing:** "Tauri's updater needs a signature… This cannot be disabled." Generate keys with `tauri signer generate`. Provide `TAURI_SIGNING_PRIVATE_KEY` (and `…_PASSWORD`) at build time; "`.env` files do _not_ work!" ([updater](https://v2.tauri.app/plugin/updater/)).
- **Config:**
  - Set `bundle.createUpdaterArtifacts: true`. The value `"v1Compatible"` exists only for apps migrating from v1.
  - Set `plugins.updater.pubkey`, which must be the key content, not a file path, and `endpoints`.
  - TLS is enforced in production unless you set `dangerousInsecureTransportProtocol`.
  - On Windows, `installMode` is `passive`, `basicUi` or `quiet` ([updater](https://v2.tauri.app/plugin/updater/)).
- **Server formats:** either a static JSON file (with `version` plus per-platform `url` and `signature`) or a dynamic server that returns 204 when there is no update ([updater](https://v2.tauri.app/plugin/updater/)).
- **Losing the private key blocks future updates** for installed apps ([updater](https://v2.tauri.app/plugin/updater/)).

### B.12 Deep links, single instance, window state

- **Deep links** ([deep-linking](https://v2.tauri.app/plugin/deep-linking/)):
  - Configure schemes under `plugins.deep-link`.
  - On Linux and Windows, links arrive as command-line arguments to a **new process**. Pair with single-instance, enabling its `deep-link` feature.
  - macOS cannot register schemes at runtime, so test with an installed bundle.
  - Runtime-registered schemes "must be manually checked" because CLI arguments can be spoofed.
  - Read links with `getCurrent` / `onOpenUrl`.
- **Single instance:** "must be the first one to be registered". The callback receives `(app, args, cwd)`. On Linux it relies on DBus, so Snap and Flatpak need explicit DBus permissions ([single-instance](https://v2.tauri.app/plugin/single-instance/)).
- **Window state:** saves and restores window positions and sizes. Create windows with `visible: false` to avoid flicker. JS access needs `window-state:default` ([window-state](https://v2.tauri.app/plugin/window-state/)).

### B.13 Frontend detection of Tauri

- **Recommended:** `isTauri()` from `@tauri-apps/api/core` (since 2.0.0). It returns `true` inside a Tauri webview, and `false` "in a browser or in unit tests" ([JS core API](https://v2.tauri.app/reference/javascript/api/namespacecore/)).
- **How it works:** its implementation is `!!((globalThis as any) || window).isTauri`. It does **not** read `__TAURI_INTERNALS__`. `window.__TAURI_INTERNALS__` is what `invoke` and `convertFileSrc` call into ([`core.ts`](https://github.com/tauri-apps/tauri/blob/dev/packages/api/src/core.ts)).
- **`window.__TAURI__`** exists only with `app.withGlobalTauri: true` ([config reference](https://v2.tauri.app/reference/config/)).
- **Lavender pattern:** one `Transport` interface on the frontend, selected once with `isTauri()`. `HttpTransport` uses fetch and WebSocket; `TauriTransport` uses `invoke` and `Channel`.

### B.14 Mobile notes

- **WebViews:** Android uses the device's System WebView (Chromium); iOS uses WKWebView, bound to the OS ([webview versions](https://v2.tauri.app/reference/webview-versions/)).
- **Dev server:** must listen on the address the CLI provides (`TAURI_DEV_HOST`). For physical devices it must be exposed on the local network ([develop](https://v2.tauri.app/develop/)).
- **Tauri 2.12 changes:** faster mobile dev loading, and an Android template with Gradle 9, Kotlin 2 and target SDK 37 ([2.12 blog](https://v2.tauri.app/blog/tauri-2.12/)).
- **Plugins:** several are desktop-only in practice. Single-instance and window-state are `#[cfg(desktop)]` in their setup docs ([single-instance](https://v2.tauri.app/plugin/single-instance/); [window-state](https://v2.tauri.app/plugin/window-state/)).
- **Memory:** a WKWebView web-content process has no fixed, settable memory limit; it "varies with device RAM and current system load" (Apple Developer Forums, community answer, [thread 133449](https://developer.apple.com/forums/thread/133449)). This makes heavy 3D Tiles on iOS a real risk.

### B.15 Minimal correct Tauri 2 layout for Lavender

These files follow the official templates: the CLI app template ([Cargo manifest](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/templates/app/src-tauri/Cargo.crate-manifest), [lib.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/templates/app/src-tauri/src/lib.rs), [main.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/templates/app/src-tauri/src/main.rs), [capabilities/default.json](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/templates/app/src-tauri/capabilities/default.json)), create-tauri-app ([tauri.conf.json template](https://github.com/tauri-apps/create-tauri-app/blob/dev/templates/_base_/src-tauri/%25(v2)%25tauri.conf.json.lte), [Cargo.toml template](https://github.com/tauri-apps/create-tauri-app/blob/dev/templates/_base_/src-tauri/Cargo.toml.lte), [vite config](https://github.com/tauri-apps/create-tauri-app/blob/dev/templates/template-vanilla-ts/vite.config.ts.lte)), and [project structure](https://v2.tauri.app/start/project-structure/).

```
apps/desktop/src-tauri/
├── Cargo.toml
├── build.rs                 # fn main() { tauri_build::build() }
├── tauri.conf.json
├── capabilities/
│   └── default.json
├── permissions/             # optional app permission TOML files
├── icons/
└── src/
    ├── main.rs              # #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
    │                        # fn main() { lavender_desktop_lib::run() }
    └── lib.rs               # #[cfg_attr(mobile, tauri::mobile_entry_point)] pub fn run() { … }
```

`src-tauri/Cargo.toml`:
- `crate-type` must include `staticlib`, `cdylib` and `rlib` for mobile.
- The `_lib` suffix avoids a Windows bin/lib name clash ([cargo#8519](https://github.com/rust-lang/cargo/issues/8519), cited in the template).

```toml
[package]
name = "lavender-desktop"
version = "0.1.0"
edition = "2024"
rust-version = "1.90"          # Tauri 2.12 MSRV

[lib]
name = "lavender_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
lavender-core = { path = "../../../crates/lavender-core" }   # adjust to the real layout

[profile.release]               # from the create-tauri-app template
codegen-units = 1
lto = true
opt-level = 3
panic = "abort"
strip = true
```

`src-tauri/tauri.conf.json`:
- `frontendDist` is "The path to the application assets… or a URL", and `devUrl` is "The URL to load in development" ([config](https://v2.tauri.app/reference/config/)).
- The paths and port below are placeholders.
- The CSP is explained in D.3. Its host list is a starting point, **UNVERIFIED** as complete, so capture the real hosts from a network log.

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Lavender",
  "version": "0.1.0",
  "identifier": "systems.noir.lavender",
  "build": {
    "devUrl": "http://localhost:5173",
    "frontendDist": "../../web/dist"
  },
  "app": {
    "windows": [
      { "label": "main", "title": "Lavender", "width": 1440, "height": 900, "visible": false }
    ],
    "security": {
      "csp": {
        "default-src": "'self'",
        "script-src": "'self' 'wasm-unsafe-eval'",
        "style-src": "'self' 'unsafe-inline'",
        "worker-src": "'self' blob:",
        "img-src": "'self' data: blob: https://api.cesium.com https://assets.ion.cesium.com https://tile.googleapis.com",
        "connect-src": "'self' ipc: http://ipc.localhost https://api.cesium.com https://assets.ion.cesium.com https://tile.googleapis.com",
        "object-src": "'none'",
        "base-uri": "'self'",
        "frame-ancestors": "'none'"
      },
      "dangerousDisableAssetCspModification": ["style-src"]
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```

`build.beforeDevCommand` and `build.beforeBuildCommand` are optional shell commands that run before `tauri dev` and `tauri build` ([config](https://v2.tauri.app/reference/config/)). The Vite side should use a fixed port with `strictPort`, honour `TAURI_DEV_HOST`, and ignore `**/src-tauri/**` ([vite template](https://github.com/tauri-apps/create-tauri-app/blob/dev/templates/template-vanilla-ts/vite.config.ts.lte)).

`src-tauri/capabilities/default.json`: the template grants `core:default` to window `main`. Add plugin and app-command permissions explicitly as they are introduced.

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

---

## C. WebView engines, WebGL and CesiumJS

### C.1 Engines

| Platform | Engine | Who updates it | Source |
|---|---|---|---|
| Windows | WebView2 (Chromium/Edge) | Self-updating ("evergreen") | [webview versions](https://v2.tauri.app/reference/webview-versions/) |
| macOS / iOS | WKWebView (WebKit) | OS updates only. "unsupported macOS versions **do not** receive WebKit updates" | same |
| Linux | WebKitGTK (Tauri 2 requires `libwebkit2gtk-4.1`) | Distro packages | same; [prerequisites](https://v2.tauri.app/start/prerequisites/) |
| Android | System WebView (Chromium) | Device's WebView provider | same |

The latest WebKitGTK stable release is **2.54.1** (2026-10-02). There were six WebKitGTK security advisories in 2026 through 29 Sep ([webkitgtk.org news](https://webkitgtk.org/news.html)). Users get whatever their distro ships.

### C.2 WebGL 2 support

- **Chromium:** WebGL 2 has been fully supported since Chrome 56 and Edge 79. **Safari / iOS Safari:** since version 15 ([caniuse](https://caniuse.com/webgl2)). WebKit's Safari 15 post announced WebGL 2, with WebGL running on Metal ([WebKit blog](https://webkit.org/blog/11989/new-webkit-features-in-safari-15/)).
- **WebKitGTK:**
  - Switched WebGL to ANGLE and enabled WebGL 2 in the 2.39.x development cycle; 2.40.0 (2023-03) was the first stable release with both ([2.40.0 release](https://webkitgtk.org/2023/03/17/webkitgtk2.40.0-released.html)).
  - The 2.51.1 development release made the GPU process the default for WebGL ([2.51.1 news](https://webkitgtk.org/releases/webkitgtk-2.51.1.tar.xz.news)).
- **CesiumJS requirements:** requests WebGL 2 by default and falls back to WebGL 1. Since 1.140, billboards and labels need WebGL 2, or WebGL 1 with `ANGLE_instanced_arrays` ([CHANGES.md](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)).

### C.3 Linux (WebKitGTK) GPU, DMA-BUF and compositing problems

**Symptoms** listed in Tauri's official guide ([Linux graphics issues](https://v2.tauri.app/develop/debug/linux-graphics/); origin issue [tauri#9394](https://github.com/tauri-apps/tauri/issues/9394)):
- A blank or white window.
- Flicker, and crashes on resize.
- `AcceleratedSurfaceDMABuf was unable to construct a complete framebuffer`.
- Wayland `Error 71`.

**Cause:** mostly the DMA-BUF renderer asking NVIDIA for buffer formats it doesn't provide ([WebKit bug 261874](https://bugs.webkit.org/show_bug.cgi?id=261874)).

**Workarounds, in the guide's order:**
1. Enable kernel mode setting (`nvidia_drm.modeset=1` for drivers older than 545).
2. Set `__NV_DISABLE_EXPLICIT_SYNC=1`.
3. Set `WEBKIT_DISABLE_DMABUF_RENDERER=1`. This fixes the framebuffer error and Error 71, but drops the fast path.
4. Last resort: `WEBKIT_DISABLE_COMPOSITING_MODE=1`, which "disables accelerated compositing entirely".

**Setting them in code:** you can set these in `main()` before the webview starts, guarded to Linux. The guide warns: "Only ship an unconditional override like this if you have verified your app is affected" ([guide](https://v2.tauri.app/develop/debug/linux-graphics/)).

**Third-party reports, not in Tauri's docs:**
- `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1` fixed an AMD transparency case where the DMA-BUF workaround did not help ([block/buzz#2643](https://github.com/block/buzz/issues/2643)).
- GeoLibre's shipping Tauri app picks workarounds based on the WebKitGTK version and on whether the GPU is NVIDIA ([lib.rs](https://github.com/opengeos/GeoLibre/blob/main/apps/geolibre-desktop/src-tauri/src/lib.rs)):
  - On WebKitGTK ≥ 2.52 it sets `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`, describing it as "a modern shared-memory fallback that avoids both the blank window and the slow legacy renderer".
  - It reserves `WEBKIT_DISABLE_DMABUF_RENDERER=1` for older WebKitGTK.
  - **UNVERIFIED** against WebKitGTK's own documentation.

### C.4 Silent slowness and other Linux hazards

- **Silent software fallback** ([guide](https://v2.tauri.app/develop/debug/linux-graphics/)):
  - WebGL2 context creation can succeed on a software rasteriser or a slow presentation path, so no error is raised.
  - WebKitGTK masks the renderer string: `WEBGL_debug_renderer_info` "reports `Apple GPU` on every Linux machine".
  - Tauri's recommended mitigation is to "give it a non WebGL fallback on Linux and consider exposing a setting". **CesiumJS has no non-WebGL fallback**, so this mitigation doesn't apply to Lavender's globe.
- **Open issues:**
  - [tauri#6559](https://github.com/tauri-apps/tauri/issues/6559) ("WebGL: Context lost") is still open, with a January 2026 confirmation using Bevy on Linux Mint 22. The suggested workaround was "CEF or Electron".
  - [WebKit bug 278695](https://bugs.webkit.org/show_bug.cgi?id=278695) (GTK WebGL slow frame rate on a 3D city map) is still NEW.
- **WebAssembly crash:** on x86-64 CPUs without AVX, JavaScriptCore's WASM tier-up crashes the web process with SIGILL. GeoLibre disables the JSC OSR-entry options on such CPUs and verified this against WebKitGTK 2.52.6 ([GeoLibre#2087](https://github.com/opengeos/GeoLibre/issues/2087); [lib.rs](https://github.com/opengeos/GeoLibre/blob/main/apps/geolibre-desktop/src-tauri/src/lib.rs)). Cesium uses WASM for Draco, KTX2 and meshopt ([cesium#13617](https://github.com/CesiumGS/cesium/issues/13617)), so this applies to Lavender.
- **Way out: a bundled Chromium.**
  - Tauri maintainers have a CEF runtime: first on a `feat/cef` branch, now `tauri-runtime-cef` in the v3 alphas ([tauri#14963](https://github.com/tauri-apps/tauri/issues/14963); [v3 alpha.0 notes](https://github.com/tauri-apps/tauri/releases/tag/tauri-v3.0.0-alpha.0)).
  - Readest reports that "every Linux build" (AppImage, deb, rpm, Flathub, Nix) has run on the Tauri CEF runtime since 0.12.8. It "now runs as smoothly as on macOS and Windows" ([comment, 2026-10-03](https://github.com/tauri-apps/tauri/issues/14963#issuecomment-5965777662)).
  - Costs: a maintainer says CEF instances are **not shared across apps** in the foreseeable future ([comment, 2026-05-18](https://github.com/tauri-apps/tauri/issues/14963#issuecomment-4479260324)), so each app ships its own Chromium. It is also alpha.

### C.5 Memory limits

- **WKWebView:** no fixed or raisable limit. It depends on device RAM and load. A memory-pressure warning usually arrives first, and `webViewWebContentProcessDidTerminate` gives no reason (Apple Developer Forums community answers: [133449](https://developer.apple.com/forums/thread/133449), [766309](https://developer.apple.com/forums/thread/766309)).
- **WebView2:** exposes `MemoryUsageTargetLevel` (`Low` is best-effort, for inactive apps) and advises against disabling the GPU except when troubleshooting ([API reference](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.memoryusagetargetlevel); [performance guide](https://learn.microsoft.com/microsoft-edge/webview2/concepts/performance)). Wry/Tauri access to it is **UNVERIFIED**.
- **WebKitGTK:** I found no documented WebGL or texture memory limit (**UNVERIFIED**).
- **Practical control:** Cesium's own `Cesium3DTileset` memory budget (`cacheBytes` / `maximumCacheOverflowBytes`). The option names come from the Cesium API and their exact current semantics are **UNVERIFIED** here; check the [Cesium3DTileset docs](https://cesium.com/learn/cesiumjs/ref-doc/Cesium3DTileset.html).

### C.6 Workers, WebAssembly, `blob:` and CSP inside the WebView

- **Module workers:** Cesium's `TaskProcessor` creates **module workers** (`type: "module"`) from `CESIUM_BASE_URL/Workers/*.js`. It throws "This browser is not supported" if ESM workers are unavailable ([TaskProcessor.js](https://github.com/CesiumGS/cesium/blob/main/packages/engine/Source/Core/TaskProcessor.js)). That support is present in Chromium and current WebKit; exact WebKitGTK minimum versions are **UNVERIFIED**, so smoke-test on the oldest supported distro.
- **`blob:` workers:** Cesium creates `blob:` shim workers in two cases. One is when workers are cross-origin (an `import "<url>"` shim). The other is when it uses the combined `Build/Cesium/Cesium.js`, which embeds the workers ([TaskProcessor.js](https://github.com/CesiumGS/cesium/blob/main/packages/engine/Source/Core/TaskProcessor.js); [CHANGES.md 1.110](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)). With same-origin workers served from the app bundle, `worker-src 'self'` suffices. Keep `blob:` only if a smoke test shows it is needed.
- **WebAssembly:** WASM requires `'wasm-unsafe-eval'` in `script-src` once any CSP is present ([MDN script-src](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/script-src); [Tauri CSP](https://v2.tauri.app/security/csp/)).
- **Runtime styles:** CesiumJS injects a `<style>` element for credits ([CreditDisplay.js](https://github.com/CesiumGS/cesium/blob/main/packages/engine/Source/Scene/CreditDisplay.js)). See B.5 for the nonce interaction.

### C.7 CORS and fetching https from the WebView

- **Page origins:** the page origin is `tauri://localhost` on macOS and Linux and `http://tauri.localhost` on Windows (`https://` if `useHttpsScheme`). Remote servers must answer CORS for that origin ([docs.rs Builder](https://docs.rs/tauri/latest/tauri/struct.Builder.html)).
- **Measured preflights, 2026-10-11** (`curl` with an `Origin` header):

| Endpoint | `Origin: tauri://localhost` | `Origin: http://tauri.localhost` | `Origin: https://tauri.localhost` |
|---|---|---|---|
| `https://api.cesium.com/v1/assets/1/endpoint` | `ACAO: *` | `ACAO: *` (allows `authorization`) | `ACAO: *` |
| `https://tile.googleapis.com/v1/3dtiles/root.json` | origin reflected | origin reflected | origin reflected |
| `https://assets.ion.cesium.com/` | `ACAO: *` | — | — |
| `https://tile.openstreetmap.org/0/0/0.png` | `ACAO: *` | — | — |

  So Cesium ion and Google tiles are reachable from all three WebView origins as far as CORS is concerned.
- **Google key restrictions:** whether a Google API key restricted by HTTP referrer accepts `tauri://` or `tauri.localhost` referrers is **UNVERIFIED**. Plan for an unrestricted key scoped by API, or proxy through the core.
- **Same-origin providers:** for Lavender's own feeds, avoid CORS entirely. Data flows through IPC (desktop) or same-origin HTTP (web).

### C.8 Reports of CesiumJS inside embedded WebViews

- **Tauri — GeoLibre** (opengeos, about 7.9k stars). GeoLibre is a production Tauri 2 desktop app that offers CesiumJS as one of four rendering engines ([rendering engines doc](https://github.com/opengeos/GeoLibre/blob/main/docs/user-guide/rendering-engines.md)).
  - Its desktop CSP allows `script-src 'self' blob: 'unsafe-eval' 'wasm-unsafe-eval'`, `style-src 'self' 'unsafe-inline'` and `worker-src blob: 'self'` ([tauri.conf.json](https://github.com/opengeos/GeoLibre/blob/main/apps/geolibre-desktop/src-tauri/tauri.conf.json)).
  - It ships the Linux WebKitGTK and AVX workarounds described above.
- **Electron — coasys/we.** A Cesium globe went blank on 1 Oct 2026 because it used the **ion demo token compiled into the installed `cesium` package**. The fix bundled offline imagery and Cesium's workers locally, and made ion opt-in ([PR #260](https://github.com/coasys/we/pull/260); third-party). The same PR claims the 1.146 demo token works until 1 Dec 2026 (**UNVERIFIED**).
- **Electron — Cesium packaging:** a 3D tileset loaded via CDN but not once Cesium was packaged ([cesium#11487](https://github.com/CesiumGS/cesium/issues/11487)).
- **WKWebView/Safari:**
  - Safari 27 beta failed to link Cesium's globe atmosphere shader through ANGLE/Metal; it was fixed in Safari 27 Beta 5 ([cesium#13650](https://github.com/CesiumGS/cesium/issues/13650)). This shows that macOS WebKit regressions reach WKWebView apps, tied to OS updates.
  - Older forum threads report Safari being slower than Chrome for Cesium ([forum](https://community.cesium.com/t/cesium-and-safari-compatibility/14646); dated).
- **Desktop app with strict CSP:** a team evaluating Cesium "in a desktop app" asked to move WASM into workers to avoid document-level `wasm-unsafe-eval` ([cesium#13617](https://github.com/CesiumGS/cesium/issues/13617)).
- **No Cesium-specific report in tauri-apps/tauri.** Searching its issues for "cesium" returns one unrelated hit, #7883 ([GitHub search](https://github.com/tauri-apps/tauri/issues?q=cesium)).

### C.9 Risk assessment

| Platform | Risk | Why |
|---|---|---|
| Windows (WebView2) | **Low** | Evergreen Chromium; WebGL 2 since Edge 79 ([webview versions](https://v2.tauri.app/reference/webview-versions/); [caniuse](https://caniuse.com/webgl2)) |
| macOS (WKWebView) | **Medium-low** | WebGL 2 since Safari 15. The WebKit version is tied to macOS, with no updates on unsupported macOS ([webview versions](https://v2.tauri.app/reference/webview-versions/)), and OS-update regressions do happen ([cesium#13650](https://github.com/CesiumGS/cesium/issues/13650)) |
| Linux (WebKitGTK) | **High** | The engine is distro-versioned. GPU and driver failures are documented. There are silent software fallbacks, a masked renderer string, an open WebGL issue, and a WASM crash on non-AVX CPUs. Tauri's own mitigation (a non-WebGL fallback) is impossible for Cesium (sources in C.3–C.4). |
| iOS / Android | **Medium-high** (if ever targeted) | Memory limits are unpublished and device-dependent ([Apple forums](https://developer.apple.com/forums/thread/133449)) |

**Mitigations for Linux:**
- Version- and GPU-aware environment defaults that a user-set value always overrides (the GeoLibre pattern).
- A WebGL `webglcontextlost` handler with a recovery UI.
- A frame-time probe at startup that suggests "open in browser" (the web mode served by the Axum adapter) when frame times are poor.
- Treat Linux desktop as beta until the CEF runtime is stable.

---

## D. CesiumJS

### D.1 Version, cadence and licence

- **Current release:**
  - CesiumJS **1.146**, released 2026-10-01 ([CHANGES.md](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md); npm [`cesium` 1.146.0](https://www.npmjs.com/package/cesium)).
  - Packages: [`@cesium/engine` 26.4.0](https://www.npmjs.com/package/@cesium/engine), [`@cesium/widgets` 16.3.0](https://www.npmjs.com/package/@cesium/widgets).
  - **New in 1.146:** a `@cesium/core` package (math, geometry, time, utilities). Importing those classes from `@cesium/engine` is deprecated and will be removed in **1.150** ([CHANGES.md](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)).
- **Cadence:** monthly, on or about the first business day. Recent releases: 1.137 on 2026-01-05, 1.138 on 02-02, 1.139 on 03-02, then 04-01, 05-01, 06-01, 07-01, 08-01, 09-02 and 10-01, with occasional patch releases such as 1.139.1 ([CHANGES.md](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)).
- **Licence:** Apache-2.0, "free for both commercial and non-commercial use" ([@cesium/engine README](https://github.com/CesiumGS/cesium/blob/main/packages/engine/README.md); [npm](https://www.npmjs.com/package/cesium)).

### D.2 `CESIUM_BASE_URL` and static assets

- **What to copy:**
  - With the combined `cesium` package, the official Vite example copies `Build/Cesium/{ThirdParty,Workers,Assets,Widgets}` and defines `CESIUM_BASE_URL` to match the destination ([cesium-vite-example vite.config.js](https://github.com/CesiumGS/cesium-vite-example/blob/main/vite.config.js); [Cesium blog, 2024-02-13](https://cesium.com/blog/2024/02/13/configuring-vite-or-webpack-for-cesiumjs/)).
  - With engine-only usage, `@cesium/engine` 26.4.0 ships `Build/{Assets,ThirdParty,Widgets,Workers}` ([package listing](https://unpkg.com/@cesium/engine@26.4.0/?meta)). The Lavender spike already copies these four directories (`spikes/tauri-cesium/scripts/copy-cesium-assets.mjs`).
- **What's in them:**
  - `Workers`: module-worker scripts, loaded at runtime from `CESIUM_BASE_URL` ([TaskProcessor.js](https://github.com/CesiumGS/cesium/blob/main/packages/engine/Source/Core/TaskProcessor.js)).
  - `ThirdParty`: WASM binaries, for example `draco_decoder.wasm` and `basis_transcoder.wasm` ([cesium#13617](https://github.com/CesiumGS/cesium/issues/13617)).
  - `Assets`: textures, sky box and approximate terrain heights.
  - `Widgets`: CSS and images.

### D.3 CSP: `unsafe-eval`, `wasm-unsafe-eval`, inline styles

- **`'unsafe-eval'` comes from Knockout.**
  - Cesium vendors Knockout (`knockout-3.5.1.js`, `knockout-es5.js`) in `@cesium/widgets` ([directory](https://github.com/CesiumGS/cesium/tree/main/packages/widgets/Source/ThirdParty)).
  - A Cesium maintainer (2026-09-17): "the main cesium package or the `@cesium/widgets` package has a dependency on knockout.js which requires `unsafe-eval`" ([cesium#13412](https://github.com/CesiumGS/cesium/issues/13412)).
  - Removing Knockout looks like "significant work" ([cesium#13617 comment](https://github.com/CesiumGS/cesium/issues/13617)), and replacing it is an open topic. Older forum threads give the same diagnosis ([forum](https://community.cesium.com/t/using-cesium-with-csp-settings/12663)).
- **Avoiding it:** use `@cesium/engine` (plus `@cesium/core`) with **`CesiumWidget`** (`import { CesiumWidget } from "@cesium/engine"`), not `Viewer` or the combined `cesium` package ([engine README](https://github.com/CesiumGS/cesium/blob/main/packages/engine/README.md)). You lose the Knockout widgets: the timeline, animation, geocoder UI, base-layer picker and InfoBox. Lavender would build its own UI anyway.
- **`'wasm-unsafe-eval'` is still required in the document today.**
  - [cesium#13412](https://github.com/CesiumGS/cesium/issues/13412) (open) reports CSP errors after upgrading to 1.139.
  - [PR #13669](https://github.com/CesiumGS/cesium/pull/13669) ("Optionally move WebAssembly to workers") is **open, not merged**. It would let `@cesium/engine` run without document-level `wasm-unsafe-eval`; a prerelease is published as [`@cesium/engine@26.3.1-wasm.0`](https://www.npmjs.com/package/@cesium/engine?activeTab=versions) (npm dist-tag `wasm`).
  - Even then, the default SPZ and Basis decoders still need `'unsafe-eval'` *inside their worker* because of Emscripten glue ([cesium#13617](https://github.com/CesiumGS/cesium/issues/13617)).
- **Inline styles:** `CreditDisplay` injects a `<style>` element with no nonce option. A PR adding a nonce was closed unmerged ([PR #13137](https://github.com/CesiumGS/cesium/pull/13137); [CreditDisplay.js](https://github.com/CesiumGS/cesium/blob/main/packages/engine/Source/Scene/CreditDisplay.js)). So `style-src` needs `'unsafe-inline'` (see B.5 for the Tauri nonce interaction).
- **The current spike's CSP** (`spikes/tauri-cesium/src-tauri/tauri.conf.json`) is `script-src 'self'` and `style-src 'self'`. Two consequences are my inference from the sources above and should be confirmed in the spike:
  - It should block Cesium's WASM decoders, which affects Draco- or KTX2-compressed 3D Tiles and glTF.
  - It should block the credit stylesheet.
- **The spike also hides credits** (`#credits { display: none; }` in `spike.css`). That is incompatible with Google and Cesium ion attribution requirements (D.5).

### D.4 Vite integration in 2026

- **Recommended: `vite-plugin-static-copy`.** It is what the official [cesium-vite-example](https://github.com/CesiumGS/cesium-vite-example) uses. Current version 4.1.1, with peer `vite ^6 || ^7 || ^8` ([npm](https://www.npmjs.com/package/vite-plugin-static-copy)). A plain copy script into `public/`, as the spike does, is equivalent.
- **Not recommended: `vite-plugin-cesium`.** Its last publish was 1.2.23 on 2024-08-06, and the repo was last pushed 2024-09-11 ([npm](https://www.npmjs.com/package/vite-plugin-cesium); [repo](https://github.com/nshen/vite-plugin-cesium)). It peers on the combined `cesium` package, so it is stale and points at the Knockout-carrying package.
- **Vite** is at 8.3.4 ([npm](https://www.npmjs.com/package/vite)).

### D.5 Google Photorealistic 3D Tiles

- **API:** `createGooglePhotorealistic3DTileset({ key, onlyUsingWithGoogleGeocoder })`. The docs state: "Google Photorealistic 3D Tiles can only be used with the Google geocoder" ([ref doc 1.146](https://cesium.com/learn/cesiumjs/ref-doc/global.html#createGooglePhotorealistic3DTileset)).
- **Default routing:** without a key, tiles are served **via Cesium ion**, and no demo Google key ships (since 1.110.1). Since 1.111, credits are linked rather than shown on screen by default, as "compliant with the minimum required attribution" ([CHANGES.md](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)).
- **Google policy** ([Map Tiles API policies](https://developers.google.com/maps/documentation/tile/policies)):
  - Aggregate the `copyright` attributions of all displayed tiles and show them in one line.
  - Show the Google Maps logo (16–19dp).
  - In CesiumJS, enable `showCreditsOnScreen`.
  - "you must not pre-fetch, index, store, or cache any Content" except as allowed. Honour `Cache-Control`. **Offline use is prohibited.**
  - Overlays may not be traced or derived from the tiles.
- **Quotas:** via ion, Google 3D Tiles are metered in **root tiles per month**: Community 1,000, Commercial 5,000, Premium 10,000 ([ion pricing](https://cesium.com/platform/cesium-ion/pricing/)).
- **Implication for desktop:** the core must not cache Google tile content beyond what HTTP caching allows, and offline mode cannot include it.

### D.6 Cesium ion tokens and terms

- **The default token is for evaluation only.** `Ion.defaultAccessToken`: "A default access token is provided for evaluation purposes only" ([Ion ref](https://cesium.com/learn/cesiumjs/ref-doc/Ion.html)). Apps that relied on the token baked into a release went blank when it rotated ([coasys/we#260](https://github.com/coasys/we/pull/260), third-party).
- **Lavender should never depend on it.** Use a per-deployment token on the server, or a user-supplied token stored in the keychain on desktop. GeoLibre's desktop build requires the user's own token ([GeoLibre docs](https://github.com/opengeos/GeoLibre/blob/main/docs/user-guide/rendering-engines.md)).
- **Plans** ([pricing](https://cesium.com/platform/cesium-ion/pricing/)):

| Plan | Price | Use | Streaming | Global imagery sessions |
|---|---|---|---|---|
| Community | Free | "Personal and non-commercial use" (exploratory commercial development allowed) | 15 GB/month | 1,000/month |
| Commercial | $149/month (individual) | Commercial use | 150 GB/month | 5,000/month |
| Premium | $499/month (individual) | Commercial use | 500 GB/month | 10,000/month |

- **Full terms of service:** not reviewed (**UNVERIFIED**). Read [cesium.com/legal](https://cesium.com/legal/) before shipping.

### D.7 Performance for many dynamic points

- **Prefer primitives over the Entity API for large, frequently-updated sets.** Cesium added `GeoJsonPrimitive` specifically to bypass "the entity/DataSource layer for significantly improved performance with large datasets" ([CHANGES.md 1.142](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)). A specific point-count threshold is **UNVERIFIED**; measure it.
- **`PointPrimitiveCollection` / `BillboardCollection`** ([PointPrimitiveCollection](https://cesium.com/learn/cesiumjs/ref-doc/PointPrimitiveCollection.html)):
  - "prefer a few collections, each with many points, to many collections with only a few points each".
  - Group points by update frequency.
  - `BlendOption.OPAQUE` or `TRANSLUCENT` "can improve performance by up to 2x".
- **`BufferPointCollection`** (and the polyline and polygon variants) are **experimental** since 1.140 ([BufferPointCollection](https://cesium.com/learn/cesiumjs/ref-doc/BufferPointCollection.html); [CHANGES.md](https://github.com/CesiumGS/cesium/blob/main/CHANGES.md)):
  - ArrayBuffer storage with fixed capacity (`primitiveCountMax`).
  - Picking is off by default.
  - Batch `setPositions()` was added in 1.146.
  - A precomputed `boundingVolume` helps large animated sets.
  - Being experimental, it is "subject to change without Cesium's standard deprecation policy".
- **`requestRenderMode: true`** cut idle CPU from 25.1% to 3.0% in Cesium's test. Call `scene.requestRender()` after Primitive or Entity changes. Set `maximumRenderTimeChange` to `Infinity` if nothing depends on simulation time ([Cesium blog](https://cesium.com/blog/2018/01/24/cesium-scene-rendering-performance/)).
- **Applying it to live feeds:** batch updates per feed tick, write positions in bulk, then call `requestRender()` once.

### D.8 Alternatives

| Option | Status | Source |
|---|---|---|
| deck.gl 9.4.0 (2026-09-05) + MapLibre GL JS 6.14.0 (2026-10-11) | deck.gl has worked "seamlessly with the MapLibre v5 globe view" since 9.1. 9.4 adds `@deck.gl/maplibre` and gives GlobeView terrain, 3D Tiles and pitch support. `GlobeView` is still **experimental**: no high-precision rendering above zoom 12, and Heatmap, Contour and Terrain layers are unsupported. MapLibre enables the globe with `map.setProjection({ type: 'globe' })`. | [deck.gl what's new](https://deck.gl/docs/whats-new); [GlobeView](https://deck.gl/docs/api-reference/core/globe-view); [MapLibre globe example](https://maplibre.org/maplibre-gl-js/docs/examples/display-a-globe-with-a-vector-map/); npm [deck.gl](https://www.npmjs.com/package/deck.gl), [maplibre-gl](https://www.npmjs.com/package/maplibre-gl) |
| iTowns 2.46.0 (last published 2025-10-03, licence `CECILL-B OR MIT`) | Globe and planar viewers, 3D Tiles, MVT, raster/COG, point clouds; associated with IGN | [itowns-project.org](https://www.itowns-project.org/); [npm](https://www.npmjs.com/package/itowns) |

**Comparison.** CesiumJS remains the most complete fit for Lavender's 3D globe: a true WGS84 ellipsoid, terrain, 3D Tiles (including Google Photorealistic), time-dynamic CZML and satellite-scale camera ranges, all Apache-2.0. Its costs are a large bundle, WebGL-only rendering (fatal on bad Linux WebKitGTK stacks) and CSP friction (Knockout and WASM). deck.gl with MapLibre's globe is lighter and better for dense 2D/2.5D analytic layers. Its globe is still experimental, with no precision above zoom 12, so it suits overview globes and 2D map modes rather than precise 3D. It is also WebGL/WebGPU-based, so it doesn't solve the Linux risk. iTowns is capable but has a much smaller ecosystem and a slower release cadence (last npm publish a year ago). A sensible hedge is to keep Lavender's layer model renderer-agnostic, as GeoLibre does across four engines ([GeoLibre](https://github.com/opengeos/GeoLibre/blob/main/docs/user-guide/rendering-engines.md)), with Cesium for 3D and MapLibre (± deck.gl) for 2D.

---

## E. Rust crates (versions from crates.io, 2026-10-11)

| Crate | Version (released) | Licence | Rationale / notes |
|---|---|---|---|
| [axum](https://crates.io/crates/axum) | **0.8.9** (2026-04-14) | MIT | Tokio-team HTTP framework on tower and hyper. The `ws` feature uses `tokio-tungstenite ^0.29` ([deps](https://crates.io/crates/axum/0.8.9/dependencies)). |
| [tokio](https://crates.io/crates/tokio) | **1.53.2** (2026-10-03) | MIT | Runtime, shared with Tauri (`tauri::async_runtime`). |
| [tower](https://crates.io/crates/tower) | **0.5.3** (2026-01-12) | MIT | Service and Layer abstractions; axum depends on `^0.5.2`. |
| [tower-http](https://crates.io/crates/tower-http) | **0.7.1** (2026-08-31) | MIT | CORS, compression, tracing, timeouts, `ServeDir` ([CHANGELOG](https://github.com/tower-rs/tower-http/blob/main/tower-http/CHANGELOG.md)). axum 0.8.9 declares an *optional* dependency on `tower-http ^0.6.8`, so 0.6 may also compile depending on axum features. |
| [hyper](https://crates.io/crates/hyper) | 1.12.0 (2026-10-06) | MIT | Indirect only, via axum and reqwest; don't depend on it directly. |
| [reqwest](https://crates.io/crates/reqwest) | **0.13.5** (2026-09-08) | MIT/Apache-2.0 | 0.13 made **rustls the default** (aws-lc provider, platform verifier), renamed `rustls-tls` to `rustls`, and moved `query`/`form` behind features ([CHANGELOG](https://github.com/seanmonstar/reqwest/blob/master/CHANGELOG.md)). `tauri` 2.12.3 depends on `reqwest ^0.13`; `tauri-plugin-http` is still on 0.12. |
| [serde](https://crates.io/crates/serde) / [serde_json](https://crates.io/crates/serde_json) | 1.0.229 / 1.0.154 | MIT/Apache-2.0 | Serialization, required by Tauri IPC and axum JSON. |
| [tracing](https://crates.io/crates/tracing) / [tracing-subscriber](https://crates.io/crates/tracing-subscriber) | 0.1.44 / 0.3.23 | MIT | Structured logs and spans in both adapters. |
| [thiserror](https://crates.io/crates/thiserror) | **2.0.21** | MIT/Apache-2.0 | Typed errors in `lavender-domain` and `-core`; the pattern Tauri documents for command errors ([Calling Rust](https://v2.tauri.app/develop/calling-rust/)). |
| [anyhow](https://crates.io/crates/anyhow) | 1.0.104 | MIT/Apache-2.0 | Binaries and tests only; never in library APIs. |
| [moka](https://crates.io/crates/moka) | **0.12.16** (2026-08-09) | (MIT OR Apache-2.0) AND Apache-2.0 | Async TTL cache. `future::Cache::get_with` coalesces concurrent misses: "Only one of the calls evaluates its future" ([docs.rs](https://docs.rs/moka/latest/moka/future/struct.Cache.html)). This gives request coalescing for provider fetches. |
| [dashmap](https://crates.io/crates/dashmap) | 6.2.1 | MIT | Sharded concurrent map for subscription registries. Prefer moka where expiry is needed. |
| [rstar](https://crates.io/crates/rstar) | 0.13.0 (2026-05-24) | MIT/Apache-2.0 | R*-tree for bbox and nearest queries. **`geo` 0.33.1 depends on `rstar ^0.12`** ([deps](https://crates.io/crates/geo/0.33.1/dependencies)). To index `geo-types` directly, use `rstar 0.12` with geo-types' matching feature; otherwise expect two rstar versions. |
| [geo](https://crates.io/crates/geo) / [geo-types](https://crates.io/crates/geo-types) | 0.33.1 / 0.7.20 | MIT/Apache-2.0 | Geodesic and Haversine measures, predicates, simplification. |
| [sgp4](https://crates.io/crates/sgp4) | **2.4.0** (2026-02-23) | MIT | Pure-Rust port of the Celestrak reference SGP4. Differences are below algorithm accuracy; `no_std` capable ([README](https://github.com/neuromorphicsystems/sgp4)). Alternative: [satkit](https://crates.io/crates/satkit) 0.24.1 for fuller astrodynamics. |
| [tokio-tungstenite](https://crates.io/crates/tokio-tungstenite) | 0.30.0 (2026-07-11) | MIT | For **outbound** upstream WebSocket feeds. Server-side WebSockets use axum `ws`, which pulls 0.29, so pin 0.29 too to avoid two copies. |
| [futures](https://crates.io/crates/futures) / [bytes](https://crates.io/crates/bytes) / [tokio-util](https://crates.io/crates/tokio-util) | 0.3.34 / 1.12.1 / 0.7.20 | MIT(/Apache) | `Stream` combinators for `subscribe()`; zero-copy buffers; `CancellationToken` for cancellation-correct subscriptions (ADR 0002). |
| Time: [jiff](https://crates.io/crates/jiff) / [chrono](https://crates.io/crates/chrono) / [time](https://crates.io/crates/time) | 0.2.38 / 0.4.45 / 0.3.55 | Unlicense/MIT; MIT/Apache; MIT/Apache | **jiff** has the most correct time-zone handling, but is pre-1.0: "I don't currently have a timeline for a Jiff 1.0 release" ([README](https://github.com/BurntSushi/jiff)). ts-rs 12 has `jiff-impl` and `chrono-impl` features but **no `time-impl`** ([features](https://crates.io/crates/ts-rs/12.0.1)). Recommendation: **chrono** for UTC timestamps in domain types (broad ecosystem, ts-rs support), or jiff if you accept 0.x churn. |
| Config: [figment](https://crates.io/crates/figment) / [config](https://crates.io/crates/config) | 0.10.19 (2024-05-17) / 0.15.27 (2026-09-30) | MIT/Apache | figment has had no release in ~29 months, though the repo was pushed 2026-10-11 ([repo](https://github.com/SergioBenitez/Figment)). Prefer **config** or plain `serde` + `toml`. Only the server adapter loads config (ADR 0002). |
| TS types: [ts-rs](https://crates.io/crates/ts-rs) vs [specta](https://crates.io/crates/specta) | **12.0.1** vs 2.0.0-rc.25 (stable 1.0.5) | MIT | ts-rs is stable and already chosen in ADR 0002. specta 2 and [tauri-specta](https://crates.io/crates/tauri-specta) (2.0.0-rc.25) are still release candidates. They generate typed `invoke` bindings, but they are RC-only and Tauri-specific. Use **ts-rs**, and hand-write the thin transport layer. |
| OpenAPI: [utoipa](https://crates.io/crates/utoipa) + [utoipa-axum](https://crates.io/crates/utoipa-axum) | **6.0.0** (2026-09-22) + 0.3.0 (requires `axum ^0.8.4`, `utoipa ^6`) | MIT/Apache | Documents the web API. [utoipa-swagger-ui](https://crates.io/crates/utoipa-swagger-ui) 10.0.1 is optional. |
| SQLite: [rusqlite](https://crates.io/crates/rusqlite) / [sqlx](https://crates.io/crates/sqlx) | **0.40.2** / 0.9.0 (2026-05-21) | MIT / MIT/Apache | For a local desktop store, **rusqlite** (with the `bundled` feature, behind `spawn_blocking`) is simpler and has no compile-time database. sqlx 0.9 is async, but `tauri-plugin-sql` still uses sqlx 0.8 ([Cargo.toml](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/sql/Cargo.toml)); irrelevant if the plugin isn't used. |
| [keyring](https://crates.io/crates/keyring) | **4.2.0** (2026-08-29); [keyring-core](https://crates.io/crates/keyring-core) 1.0.0 | MIT/Apache | OS keychain for the desktop `SecretStore` (B.7). GeoLibre pins 3.6.3. Evaluate the 4.x API and per-platform store crates before choosing ([README](https://github.com/open-source-cooperative/keyring-rs)). |
| [governor](https://crates.io/crates/governor) + [tower_governor](https://crates.io/crates/tower_governor) | 0.10.4 + 0.8.0 (requires `axum ^0.8`, `governor ^0.10`) | MIT; MIT/Apache | GCRA rate limiting: inbound on the server via tower, and outbound per provider inside the core. |
| [csv](https://crates.io/crates/csv) / [quick-xml](https://crates.io/crates/quick-xml) | 1.4.0 / 0.42.0 | Unlicense/MIT; MIT | CSV feeds; XML/CAP/KML/RSS with serde support. |
| GRIB: [grib](https://crates.io/crates/grib) vs [eccodes](https://crates.io/crates/eccodes) | **0.19.1** (2026-10-10) vs 0.15.1 | MIT/Apache vs Apache-2.0 | **grib** is a pure-Rust GRIB2 reader ([README](https://github.com/noritada/grib-rs)). Simple packing (5.0), complex packing (5.2/5.3) and run-length (5.200) are built in. JPEG2000 (5.40), PNG (5.41) and CCSDS (5.42) need features; JPEG2000 and CCSDS each have a pure-Rust backend option (`hayro`, `rust-aec`). **eccodes** binds the ECMWF C library. It is read-only and "officially supports mainly Linux", and needs `libeccodes` installed ([README](https://github.com/ScaleWeather/eccodes)), which is bad for a Tauri desktop bundle. GRIB1 support in `grib` is **UNVERIFIED** (the README describes GRIB2). |
| Protobuf (GTFS-rt): [prost](https://crates.io/crates/prost) + [prost-build](https://crates.io/crates/prost-build) | 0.14.4 | Apache-2.0 | prost-build "depends on the Protocol Buffers compiler, `protoc`" (or set `PROTOC`) ([docs.rs](https://docs.rs/prost-build/latest/prost_build/)). Alternative: the [gtfs-realtime](https://crates.io/crates/gtfs-realtime) crate (0.2.0, 2025-07-21, built on prost 0.14), or check in generated code to avoid `protoc` in CI. |
| [geojson](https://crates.io/crates/geojson) | **1.0.0** (2026-03-16) | MIT/Apache | First 1.0 release; converts to and from geo-types. |
| [flatgeobuf](https://crates.io/crates/flatgeobuf) | 6.0.1 | BSD-2-Clause | Streaming spatially-indexed vector files. Its HTTP range client depends on `reqwest ^0.12` ([deps](https://crates.io/crates/flatgeobuf/6.0.1/dependencies)). Built on [geozero](https://crates.io/crates/geozero) 0.15.1. |

Tauri crates are listed in B.1. [proj](https://crates.io/crates/proj) 0.31.0 binds C PROJ; avoid it unless a datum transform requires it, because it adds a native dependency.

---

## F. Recommendations for Lavender

1. **Frontend: `@cesium/engine` + `@cesium/core` + `CesiumWidget`, never `Viewer` or the `cesium` package.**
   - This removes Knockout and therefore `'unsafe-eval'` (D.3).
   - Copy `Build/{Workers,ThirdParty,Assets,Widgets}` from `@cesium/engine`, using `vite-plugin-static-copy` 4.x or the existing script, and set `CESIUM_BASE_URL` (D.2, D.4).
   - Build the UI controls yourself.
   - Keep the credit container **visible**: Google and ion attribution are contractual (D.5, D.6).
2. **One CSP for web and desktop:**
   - `script-src 'self' 'wasm-unsafe-eval'` and `style-src 'self' 'unsafe-inline'` (Cesium's credit `<style>`).
   - `worker-src 'self'` (add `blob:` only if a smoke test needs it).
   - An explicit `connect-src` / `img-src` host list. In Tauri, add `ipc: http://ipc.localhost`.
   - Keep `index.html` free of inline style and script, or set `dangerousDisableAssetCspModification: ["style-src"]` (B.5).
   - Track [PR #13669](https://github.com/CesiumGS/cesium/pull/13669) to drop `wasm-unsafe-eval` from the document later.
3. **Rendering of live feeds:**
   - Use `PointPrimitiveCollection` / `BillboardCollection` grouped by feed and update rate, with `BlendOption.OPAQUE` where possible.
   - Turn on `requestRenderMode`, with one `requestRender()` per batch.
   - Put a thin `PointLayer` abstraction in front, so the experimental `BufferPointCollection` with `setPositions()` can be swapped in after profiling (D.7).
   - Avoid the Entity API for high-cardinality feeds.
4. **Rust stack, matching ADR 0002:**
   - Server: axum 0.8.9 + tower 0.5 + tower-http 0.7 + tower_governor 0.8 + utoipa 6 / utoipa-axum 0.3.
   - Core: tokio 1.53, reqwest 0.13 (rustls), moka 0.12 (TTL + coalescing), tokio-util `CancellationToken`, futures streams, tracing.
   - Domain: serde, thiserror 2, chrono 0.4 (or jiff 0.2), ts-rs 12.
   - Geo: geo 0.33 + rstar 0.12 (to match geo), sgp4 2.4, geojson 1.0, flatgeobuf 6.
   - Feeds: quick-xml 0.42, csv 1.4, prost 0.14 (GTFS-rt), grib 0.19 for GRIB2 with pure-Rust JPEG2000 and CCSDS backends where needed.
   - Desktop store: rusqlite 0.40 (`bundled`).
   - Secrets: keyring 4.2.
   - Avoid: anyhow in libraries, figment, eccodes, proj, specta RC.
5. **Desktop (Tauri 2.12.x, MSRV 1.90):**
   - Expose only app commands (snapshot, subscribe, status, settings, secrets), with `AppManifest::commands`-generated permissions granted per window.
   - Stream with `tauri::ipc::Channel`.
   - Use **no** `http`, `sql`, `store`, `fs` or `shell` plugins in the WebView, and no Node sidecar.
   - Persist settings in `app_config_dir()` through the core's `SettingsStore`.
   - Keep secrets in the OS keychain.
   - Plugins: single-instance first, deep-link with single-instance's `deep-link` feature, window-state with hidden-until-restored windows, and the updater with signing keys held only in CI secrets (B.10–B.12).
6. **Treat Linux desktop as the highest-risk target.**
   - Ship version- and GPU-aware WebKitGTK environment defaults that an explicit user value overrides, plus the non-AVX WASM guard. Both follow GeoLibre's precedent (C.3, C.4).
   - Add a `webglcontextlost` handler and a startup frame-time probe that offers the web mode in the system browser.
   - Prototype `tauri-runtime-cef` on Linux in the v3 alpha line, but don't adopt it until 3.0 is stable (C.4).
   - Windows and macOS are acceptable on system WebViews (C.9).
7. **Cesium ion and Google:**
   - Never rely on the ion demo token. Use a per-deployment token for web and a user-supplied token in the keychain for desktop.
   - Budget for the Commercial plan if Lavender is used commercially.
   - Treat Google Photorealistic 3D Tiles as online-only, attribution-on-screen, and not cacheable by the core (D.5, D.6).
   - Verify the API-key referrer behaviour from `tauri://` origins before release (C.7).
8. **Keep renderer independence.** A renderer-agnostic layer model lets a MapLibre (+deck.gl) 2D mode act as a fallback where WebGL 3D is unreliable (D.8).
9. **Agentic workflow for the rebuild** (section A):
   - Keep a JSON feature and test ledger, `progress.md`, and an `init.sh`.
   - Do one feature per session, with commits as checkpoints.
   - Verify by screenshot and tests on all three WebView engines.
   - Run a fresh-context reviewer against the ADRs before merging.
