# ADR 0007 — Rust domain types are the contract; TypeScript types are generated

- Status: Accepted
- Date: 2026-10-11

## Problem

The same payloads cross two transports (HTTP/WebSocket JSON and Tauri IPC
JSON) and two languages. Hand-maintained TypeScript mirrors drift. GEV has no
typed contract at all, only per-layer normalisers in JavaScript.

## Options

- **Hand-written TS mirrors.** Simple, and drifts.
- **OpenAPI (utoipa) + a TS client generator.** Good for HTTP, but it does not
  describe WebSocket frames or Tauri commands, and adds two generators.
- **`ts-rs` derive on domain types.** Emits a `.ts` file per type from the Rust
  definitions; transport-agnostic; mature (v12).
- **`specta` + `tauri-specta`.** Also generates Tauri command bindings, but is
  still a 2.0 release candidate.

## Decision

`lavender-domain` types derive `serde::{Serialize, Deserialize}` and
`ts_rs::TS`. The generated files live in
`apps/web/src/contracts/generated/` and are **committed**, so frontend
development does not require a Rust toolchain.

- Generation runs as a Rust test (`cargo test -p lavender-domain export_bindings`),
  which is how ts-rs exports. A check in CI regenerates the files and fails on
  any diff.
- Serialisation conventions: `camelCase` fields, internally tagged enums
  (`#[serde(tag = "kind")]`) so TypeScript gets discriminated unions,
  timestamps as RFC 3339 strings, and no `u64`/`i64` in contracts (TS `number`
  safety). IDs are strings.
- The frontend never imports generated files from components directly; it
  imports `src/contracts/index.ts`, which re-exports and may add branded
  helper types.

## Consequences

- A Rust change that breaks the frontend fails the TypeScript typecheck.
- Command *names* and argument shapes for Tauri are not generated. The
  `TauriPlatform` adapter is small and covered by an IPC integration test.
  `tauri-specta` can replace this once it is stable.
