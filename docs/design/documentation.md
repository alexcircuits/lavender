# Browser documentation design

The reader is an engineer learning Lavender or finding the code and commands for
one contribution. The browser view reads the existing Markdown; it does not keep
a second copy of the guides.

- **Content order:** orient on the overview, run locally, trace a real record,
  choose a task, then use references while implementing.
- **Concept:** the same quiet workspace as Lavender, adapted for sustained reading.
- **Signature:** a compact, navigable view of the real interface → shared core →
  providers relationship on the overview.
- **Sources:** Lavender's own favicon, Geist typography, tonal islands, platform/core
  boundary and the Start Here walkthrough.
- **Type:** Geist for interface and prose, Geist Mono for commands and identifiers.
  Prose gets a generous line height and a measure of roughly 65–72 characters.
- **Colour:** existing ground `#0a0a0b`, island `#161619`, ink `#fafafa`, secondary
  ink `#a1a1aa`, lavender ink `#b9a6ff` and primary fill `#6d5ae6`; CSS uses tokens.
- **Geometry:** a 248 px guide rail, flexible reading column and a compact outline.
  Mobile replaces the rail with an accessible navigation dialog. Hairlines separate
  groups; only commands, dialogs and the overview diagram need contained surfaces.
- **Motion:** existing focus, dialog and hover feedback only; no ambient animation.
- **Restraint:** no gradients, glass, decorative globe, marketing cards or new font.

Reuse Base UI, cmdk and the existing design primitives. Use mature Markdown/GFM,
syntax-highlighting and diagram components. Ship a separate Vite entry so reading
docs does not start feeds or load the globe, and documentation libraries do not
enter the workspace's initial bundle.

Verify production CSP, keyboard search/navigation, code copying, section links,
unknown-page recovery, small-screen reflow and wide-screen hierarchy in a browser.

## Implementation and maintenance

The HTML entry is `apps/web/docs/index.html`; components live in
`apps/web/src/docs/`. `content.ts` discovers Markdown and linked repository source
files through Vite. Navigation labels organize the guides; bodies remain in their
original Markdown. New archive documents are discovered automatically.

Both dev and test file scopes include the Lavender repository because docs and
Rust source live outside the web package. The production build packages those
files locally; it uses no remote content service or CDN. Full-text search reads
the archive when opened and prefers contributor guides for general matches.
Mermaid uses strict sanitization and renders only checked-in diagram definitions.

The docs have a separate entry and lazy renderer/content chunks. The existing
`scripts/check-dist.mjs` enforces CSP-safe HTML and a 250 KiB compressed initial
JavaScript budget for both the workspace and documentation. Editing Markdown
uses the existing Vite development process; no generator command is needed.

`apps/web/src/docs/content.test.ts` covers real file loading, routes, heading
slugs, relative links, evidence images and search. `apps/web/e2e/docs.spec.ts`
exercises the production Rust static server and its real CSP, including mobile
dialogs, code copying and section reloads. Browser screenshots are attached to
the Playwright report. All four existing workspace journeys remain required.
