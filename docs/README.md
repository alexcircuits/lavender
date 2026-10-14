# Lavender Development Documentation

Lavender puts aircraft, satellite orbits and earthquakes on a shared 3D globe. Its
backend is Rust, its interface is React and TypeScript, and its macOS desktop
prototype runs the same interface on the same Rust core.

**New contributor? Start with [Start Here](development/start-here.md), then
[Getting Started](development/getting-started.md).** You do not need to read the
historical specifications before running the app.

## Read in the browser

Start the web development server with `pnpm --dir apps/web run dev` from the
Lavender repository root, then open [Developer docs](http://localhost:5410/docs/).
Docs can run without the Rust server. The built app also includes `/docs/`, served
by Rust on its configured port. The workspace navigation has a documentation link.

Use `⌘K` or `Ctrl+K` to search guide text, select a section from the outline, and
copy commands from examples. Linked Rust and TypeScript files open in a source
reader using the checked-in files from this build. Specifications and historical
audits live under the expandable navigation section and join full-text search when
it opens. Markdown remains the source of truth; edits appear with Vite hot reload.

| Guide | What you will learn |
|---|---|
| [Start Here](development/start-here.md) | Current capabilities, where to read code, your first 30 minutes |
| [Getting Started](development/getting-started.md) | Installation, development, builds, checks and configuration |
| [Architecture](development/architecture.md) | How a request reaches a provider and returns to the globe |
| [Rust Rewrite](development/rust-rewrite.md) | Completed work, partial migrations, remaining JavaScript and parity |
| [Codebase Map](development/codebase-map.md) | Where to find each responsibility |
| [Functions and Modules](development/reference.md) | Useful entry points, inputs, results, errors and side effects |
| [Add a Data Provider](development/add-provider.md) | An end-to-end recipe using existing provider conventions |
| [Web and Desktop](development/web-and-desktop.md) | HTTP, Tauri IPC, shared code and platform differences |
| [Testing and Debugging](development/testing.md) | Tests, logs, API failures, browser checks and parity |
| [Contributing](development/contributing.md) | Choosing a Flokzy task, review workflow and definition of done |
| [Roadmap](roadmap.md) | Small milestones, dependencies and what to work on next |
| [Task Index](development/task-index.md) | A dated Flokzy snapshot with links; refresh status before starting |
| [Glossary](development/glossary.md) | Feed, provider, region, snapshot, freshness and orbital terms |
| [Verification](development/verification.md) | Evidence and limitations of the latest engineering pass |

## Specifications and history

These documents preserve detailed requirements; proposed routes and types in them
are **targets**, not an inventory of implemented APIs. The current implementation
is described in the contributor guides above.

- [Product specification](product/specification.md), [architecture specification](architecture/overview.md),
  [route-by-route migration plan](architecture/rust-migration.md).
- [Frontend](architecture/frontend.md), [desktop](architecture/tauri.md),
  [security](architecture/security.md), [design system](design/design-system.md),
  [performance targets](performance/specification.md).
- [Architecture decisions](adr/), [upstream audits](audit/), [research](research/),
  [original task breakdown](roadmap-detail.md), [progress history](PROGRESS.md).

Flokzy's [Lavender project](https://app.flokzy.com/app/projects/3cf1d056-2bd8-4cb7-9381-c83d58fb1bb2)
holds task status and discussion. Markdown stays readable on GitHub. The browser
view ships with the existing Vite build, with no separate publishing step.
