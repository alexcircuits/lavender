# ADR 0004 — Frontend stack: React + TypeScript + Vite, with the globe outside React

- Status: Accepted
- Date: 2026-10-11

## Problem

GEV's interface is 41 000 lines of vanilla JavaScript UI plus about 80 000 lines
of layer code. Many modules hold page-scoped singleton state, layers talk
through `window` events, and one 2 350-line lifecycle manager mediates
everything (`docs/audit/upstream-layers.md` §1.4, §1.9). Lavender needs strong
component boundaries, typed contracts and predictable state. It also needs a hot
rendering path that can update tens of thousands of points without UI
re-renders, and an identical build for browsers and Tauri.

## Options

| Option | For | Against |
|---|---|---|
| Modernise GEV's vanilla JS | No framework dependency | The coupling is the problem; component boundaries, accessibility and state would all be hand-rolled again |
| **React 19** | Used by Flokzy and KVB News (shared skills across the company); mature accessible headless primitives (Base UI, which Flokzy uses); good virtualisation; works unchanged in Tauri | Re-render discipline needed; must keep Cesium out of the render cycle |
| Solid / Svelte | Finer-grained reactivity, smaller runtime | No other NoirSystems product uses them; smaller primitive ecosystem |
| Web Components (Lit) | Framework-free interop | Weak story for complex app state and accessible composite widgets |

## Decision

- **React 19 + TypeScript (strict) + Vite.** TypeScript runs with `strict`,
  `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` and
  `verbatimModuleSyntax`, the strictest settings already used in the monorepo
  (`products/crypto-ide`).
- **The globe lives outside React.** A `GlobeEngine` class owns the
  `CesiumWidget`, the camera, picking and one renderer per layer. React mounts it
  once into a host element and talks to it through typed commands and
  subscriptions. Feed updates flow core → platform → layer renderer without
  touching React. React only sees small summaries (counts, freshness, the
  selected entity).
- **State:** Zustand stores (small, framework-agnostic, subscribable from the
  engine as well as from components). There are separate stores for workspace
  (layers, selection, camera bookmarks), feed status, and UI chrome. No
  component reaches into another feature's store internals; features expose
  selectors.
- **Platform boundary:** every I/O call goes through a `LavenderPlatform`
  interface (ADR 0005). Components never import `fetch`, WebSocket or
  `@tauri-apps/api`. Only `src/platform/*` may.
- **Styling:** CSS Modules plus a single design-token stylesheet (CSS custom
  properties). The token values come from the family (`docs/design/design-system.md`).
  Tailwind was considered for family consistency. CSS Modules were preferred
  because the style surface is a bounded set of stateful primitives (states via
  `data-*` attributes, `@supports (corner-shape: squircle)`) rather than many
  page layouts, and they add no dependency. This is a preference, not a
  technical necessity.
- **Primitives:** `@base-ui/react` (the headless library under Flokzy's
  components) for popovers, menus, tooltips, dialogs and switches; `cmdk` for the
  command palette (as in Flokzy); `@phosphor-icons/react` (KVB News, NoirSystems
  site); `@tanstack/react-virtual` for long lists.
- **Fonts:** Geist and Geist Mono (SIL OFL), self-hosted through Fontsource so
  the CSP stays `'self'` and the desktop build works offline.
- **Tests:** Vitest + Testing Library for units/components, Playwright for
  end-to-end, plus a rendering smoke test that checks the globe actually draws.
- **Lint/format:** Biome (as `crypto-ide`), one tool for both.

## Consequences

- One frontend for web and desktop; platform differences are confined to
  `src/platform/`.
- Engineers moving between Flokzy and Lavender share React and Base UI idioms.
- The engine/React split must be enforced in review, and with a lint rule
  forbidding `@cesium/engine` imports outside `src/globe/`.
- GEV's pure modules (orbit maths, route maths, label arbitration, coordinate
  parsing, share codec) can be ported to TypeScript almost verbatim.
