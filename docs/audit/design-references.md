# Design references for Lavender: Flokzy, KVB News, NoirSystems site, xandra-css

Audit date: 2026-10-11. Read-only research. Every token below is cited to `file:line`;
where a document and the code disagree, the code is cited and the drift is noted.

## 0. Sources and provenance caveats

Paths are relative to the monorepo root `/Volumes/NoirSystems/noir-monorepo`. Shorthand:

| Prefix | Path | Notes |
|---|---|---|
| `FZ/` | `products/flokzy/website/` | Next 16, Tailwind v4 (`@theme inline`), shadcn `base-nova` on Base UI (`FZ/components.json:3`, `FZ/package.json`). `globals.css` is byte-identical to `/Volumes/NoirSystems/work/products-flokzy` (same md5, 2026-09-19); may still be stale vs production. |
| `FZD/` | `products/flokzy/` | Flokzy design docs. |
| `KVB/` | `worktrees/kvbnews-web/products/kvbnews/web/frontend/` | Next 16, Tailwind **3**. Exists only in the `feat/kvbnews-web` worktree; `products/kvbnews` on main has no web frontend. **The Tonal Islands implementation is uncommitted** (`git status`: `M app/globals.css`, `M tailwind.config.ts`, `?? components/system/`, `?? components/signals/`, `?? lib/fonts.ts`), so line numbers may drift. |
| `KVBD/` | `worktrees/kvbnews-web/products/kvbnews/web/docs/redesign/` | KVB redesign plan, navigation/motion and Signals docs (also uncommitted). |
| `NS/` | `web/noirsystems-site/` | Next 16 on vinext/Cloudflare, Tailwind v4, mostly hand-written CSS. |
| `XC/` | `libs/xandra-css/` | npm package `xandra` 2.3.0 (`XC/package.json:2-3`). |

Things that look like tokens but are not the shipped system:

- `FZ/public/assets/tokens.css` is a static HTML prototype ("not production code", `FZD/DESIGN_INVENTORY.md:3`). Its radii (4–16), type scale and shadows differ from `FZ/src/app/globals.css`, which is what the app ships.
- `FZD/DESIGN_OVERHAUL_PLAN.md` (2026-06-10) specified radii 4/6/8/10/16 and "1px hairline `--border`" cards (`:110`, `:114`). The shipped `globals.css` moved on to a "soft & generous" radius register and borderless tonal islands (`FZ/src/app/globals.css:112-120`, `:597-602`). The shipped values are cited below.
- KVB plan §F10 motion values (`KVBD/KVB-REDESIGN-PLAN.md:669-678`: 160/240/320ms) differ from the implemented tokens (`KVB/app/globals.css:76-92`: 140/200/280ms). Implemented values are cited.
- KVB `components/ui/*` and `app/admin/*` are legacy shadcn (lucide, Radix, `oklch` greys), not yet migrated (`KVB/app/globals.css:101-102`, `KVB/app/admin/layout.tsx:24-50`). Only `components/system`, `navigation`, `signals`, `home`, `news`, `article`, `layout` carry Tonal Islands.
- In `NS/`, `.noir-console*` and `.noir-table` in `NS/app/noir.css:1229-1508` are not referenced by any TSX; the live console uses `NS/app/console/console-workspace.css` (`consolex-*`, imported at `NS/app/console/ConsoleWorkspace.tsx:52`).

---

## 1. Per-product foundations

### 1.1 Flokzy (web app)

**Model.** "Gravity-style two-layer model: raw palette → semantic tokens → components" (`FZ/src/app/globals.css:5-9`). Dark is the default; light is opt-in via `html[data-theme="light"]` (`globals.css:156-159`, `:226`; `FZ/src/lib/use-theme.ts:9-18`; anti-FOUC script `FZ/src/app/[locale]/layout.tsx:117-121`; `<html class="… dark">` at `:112`).

**Typefaces.** Inter 400–800 and JetBrains Mono 400–700, `next/font/google`, latin subset (`FZ/src/app/[locale]/layout.tsx:15-25`), mapped in `globals.css:15-17` (`--font-heading` is also Inter). Body: `font-feature-settings: "cv11","ss01","ss03"` and `letter-spacing: -0.011em` (`globals.css:306-310`). Mono is used for identifiers, keys and `kbd` (e.g. team key `FZ/src/app/[locale]/app/(dashboard)/tasks/page.tsx:473`; `.eyebrow` mono uppercase 11px on marketing only, `globals.css:401-408`).

**Type scale** (`globals.css:19-45`; lint-enforced, see below):

| Token | Size / line-height | Weight | Tracking |
|---|---|---|---|
| `text-caption-2` | 11 / 15 | — | — |
| `text-caption-1` | 12 / 16 | — | — |
| `text-body-2` | 13 / 18 | — | — |
| `text-body-1` (body) | 14 / 20 | — | −0.011em (body) |
| `text-subheader` | 16 / 22 | 600 | — |
| `text-header-2` | 20 / 26 | 600 | — |
| `text-header-1` | 26 / 32 | 600 | — |
| `text-display-2` | 36 / 40 | 600 | −0.03em |
| `text-display-1` | 56 / 58 | 600 | −0.04em |

Enforcement: custom ESLint rule `flokzy-tw/no-design-drift` bans arbitrary `text-[Npx]` and text opacity below /50 (`FZ/eslint.config.mjs:15-34`, enabled `:174`), with a bulk-suppression baseline for legacy sites (`:8-11`). A second rule bans raw z-index (`:70`, `:175`).

**Colour tokens** (dark default `globals.css:157-223`; light `:226-297`):

| Semantic | Dark | Light | Role |
|---|---|---|---|
| `--background` | `#0a0a0b` | `#f4f4f6` | canvas, shared by sidebar (`:161`, `:230`; `.dashboard-sidebar` `:608-610`) |
| `--bg-well` | `#111114` | `#e9e9ed` | sunken tray: inputs, kanban columns, empty states (`:192`, `:262`; comment `:67-69`) |
| `--card` / `--bg-subtle` | `#161619` | `#ffffff` / `#f4f4f5` | island surface (`:163`, `:191`, `:232`) |
| `--card-hover` | `#1c1c21` | `#fbfbfc` | interactive island hover (`:165`, `:234`) |
| `--popover` | `#1c1c22` | `#ffffff` | menus, dialogs (`:166`, `:235`) |
| `--secondary` / `--muted` / `--accent` | `#202026` | `#ececef` | tonal controls (`:180-185`, `:250-255`) |
| `--border` / `--input` | `#2a2a31` | `#e4e4e7` | hairline where used (`:187-188`, `:257-258`) |
| `--border-strong` | `#3f3f46` | `#d1d1d6` | (`:193`, `:263`) |
| `--foreground` | `#fafafa` | `#09090b` | (`:162`, `:231`) |
| `--muted-foreground` / `--fg-muted` | `#a1a1aa` | `#71717a` | (`:183`, `:194`, `:253`) |
| `--fg-subtle` | `#818191` | `#71717a` | (`:195`, `:265`) |
| `--destructive` | `#ef4544` | `#dc2626` | (`:186`, `:256`) |
| `--success` / `--warning` | `#45b75f` / `#f39e1e` | `#16a34a` / `#d97706` | (`:204-205`, `:274-275`) |
| `--sidebar-border` | `#26262a` | `#e4e4e7` | (`:221`, `:289`) |

Accent ramp, **Flokzy Indigo** (`globals.css:169-175` dark, `:238-244` light):

| Step | Dark | Light | Use (`FZD/DESIGN_OVERHAUL_PLAN.md:78-84`) |
|---|---|---|---|
| `accent-3` | `#1b1d3a` | `#eef0fc` | selected row / active nav bg |
| `accent-4` | `#232858` | `#e0e3fa` | hover tinted bg |
| `accent-7` | `#3f48a8` | `#8a93e8` | borders, active outlines |
| `accent-9` (= `--primary`, `--ring`) | `#5661d8` | `#4a55c9` | primary buttons, focus ring (`globals.css:177`, `globals.css:189`) |
| `accent-10` (= `--primary-hover`) | `#6b75e3` | `#3f49b4` | primary hover (`globals.css:178`) |
| `accent-11` (= `--color-info`) | `#97a0f0` | `#3a43a6` | accent text/links (`globals.css:85`) |

Charts: `#5661d8 #45b75f #f39e1e #7563ab #ef4544` (`globals.css:209-213`). Brand letter colours coral/orange/amber/green/violet/indigo (`globals.css:197-202`) are "logo + status only, never UI accent" (`globals.css:87`; logo fills `FZ/src/components/logo.tsx:20-30`).

**Accent rules.** Indigo anchored at `#5661D8` (`FZD/DESIGN_OVERHAUL_PLAN.md:64`); orange/coral/amber only in the logo and as status (`:73`); "exactly one filled accent button per screen/section" (`:112`); active nav = `accent-3` bg + `accent-11` icon (`FZ/src/components/dashboard/sidebar.tsx:182-183`).

**Radii** (`globals.css:112-120`, "soft & generous register: controls 12, cards 16–20, panels 24"): `xs 8 · sm 10 · md 12 · lg 16 · xl 20 · 2xl 24 · 3xl 28`; `--radius: 10px` (`:207`). In practice: buttons/inputs/nav items `rounded-md` 12 (`FZ/src/components/ui/button.tsx:7`; `sidebar.tsx:179`); popovers `rounded-lg` 16 (`FZ/src/components/ui/popover.tsx:20-21`); `.island` 20 (`globals.css:616-619`); dialogs and command palette `rounded-2xl` 24 (`FZ/src/components/ui/confirm-dialog.tsx:190`; `FZ/src/components/tasks/command-palette.tsx:159`).

**Squircles: none.** Plain `border-radius` everywhere; no `corner-shape`, SVG mask, `clip-path` geometry or squircle library in `FZ/src`.

**Spacing.** Tailwind's default 4px scale, no custom spacing tokens; rule "every gap a multiple of 4px" (`FZD/DESIGN_OVERHAUL_PLAN.md:113`). Shell padding `px-5 py-5 md:px-8 md:py-7` (`FZ/src/app/[locale]/app/(dashboard)/layout.tsx:44`); islands `px-4 py-2.5 md:px-5` (header) and `px-5 py-4` (stat) (`FZ/src/app/[locale]/app/(dashboard)/tasks/page.tsx:467`; `FZ/src/app/[locale]/app/(dashboard)/overview/page.tsx:66`); island groups separated by `mb-4` / `gap-4` / `space-y-6`.

**Borders / hairlines.** Intent: "blocks separate by tone, never by a hairline" (`globals.css:67-69`) and islands "carry no border" (`:597-602`). Reality: global `* { border-border }` (`:301-303`), and hairlines remain between list rows (`divide-y divide-border`, `tasks/page.tsx:735`; `FZ/src/components/tasks/task-list-status-dnd.tsx:247`), in island headers (`overview/page.tsx:265`), palette input/footer (`command-palette.tsx:162`, `:382`), sidebar groups and footer (`sidebar.tsx:247`, `:479`).

**Shadows** (`globals.css:122-129`, "overlays only; flat surfaces separate … by tone"): dark `xs 0 1px 2px 0 rgba(0,0,0,.18)` … `xl 0 12px 28px -8px rgba(0,0,0,.40), 0 4px 10px -4px rgba(0,0,0,.22)` … `2xl 0 18px 40px -12px rgba(0,0,0,.48), 0 6px 14px -6px rgba(0,0,0,.28)`; light scale `:292-297`. Islands get `shadow-xs` in light mode only (`:621-623`). Popover `shadow-xl` (`popover.tsx:20-21`), dialogs `shadow-2xl` (`confirm-dialog.tsx:190`), palette `shadow-lg` (`command-palette.tsx:159`).

**Z-index ladder** (`globals.css:131-153`): `raised 10 · sticky 100 · widget 200 · nav 300 · drawer 400 · modal 500 · popover 600 · toast 700 · tooltip 800`, with the ordering rationale written down (popover > modal so pickers inside dialogs work).

**Motion.** No duration tokens; values per component: overlays 150ms, `ease: [0.16, 1, 0.3, 1]`, modal enters from `opacity 0, scale .96, y 8` (`FZ/src/components/ui/overlay.tsx:29-32`, `:56-59`); drawer slides with a spring (damping 25, stiffness 400) (`overlay.tsx:122-126`); generic overlay spring 380/32 (`FZ/src/lib/motion.ts:29-33`); select menu fade + 4px slide, 150ms ease-out via Base UI `data-[starting-style]` (`FZ/src/components/ui/select.tsx:29-30`); `.surface-interactive` 150ms (`globals.css:627-638`); sidebar width/transform 200ms ease-out (`sidebar.tsx:371`); marketing `.card-lift` 180ms `cubic-bezier(0.16,1,0.3,1)` (`globals.css:412-419`). **Reduced motion:** global clamp to 0.01ms for all CSS animation/transition (`globals.css:316-328`) plus `useMotionTransition` returning `{duration: 0}` for Framer Motion springs (`FZ/src/lib/motion.ts:15-19`).

**Focus.** Every element gets `outline-ring/50` (`globals.css:302`); buttons `focus-visible:border-ring focus-visible:ring-3 ring-ring/50` (`button.tsx:7`); inputs `focus:ring-2 focus:ring-ring/40` (e.g. `FZ/src/app/[locale]/app/(settings)/settings/profile/page.tsx:114`). Ring colour = indigo `accent-9`.

**Density.** Body 14px; nav items `h-9` (36px) with 15px icons (`sidebar.tsx:179`, `:193`); task rows `px-3 py-2`, 24px inline icon buttons, 13–14px glyphs (`FZ/src/components/tasks/task-row.tsx:98`, `:315`); filter controls `h-7` (28px) (`FZ/src/components/tasks/task-filter-bar.tsx:338`, `:402`); buttons `xs 28 · sm 32 · default 36 · lg 40` (`button.tsx:25-37`). Overhaul plan calls the register "Gravity UI (Yandex) preset — dense, serious, enterprise dev-tool" (`FZD/DESIGN_OVERHAUL_PLAN.md:4`).

### 1.2 KVB News (web, "Tonal Islands")

**Model.** "Ground → island → inset", flat, one saturated "signal island" per viewport, red only for live (`KVB/DESIGN.md:163-172`; plan `KVBD/KVB-REDESIGN-PLAN.md:316-342`). Approved 2026-10-10 (`KVB/DESIGN.md:156`). Light and dark first-class via `next-themes` `attribute="class"`, `defaultTheme="system"` (`KVB/app/providers.tsx:15`). Colours are RGB channels (`--kvb-*`) so Tailwind 3 opacity modifiers work (`KVB/app/globals.css:7`; `KVB/tailwind.config.ts:8`).

**Typefaces.** Onest (variable weight axis, `latin`, `latin-ext`, `cyrillic`, `next/font/google`, `--font-onest`) for interface and headlines (`KVB/lib/fonts.ts:9-13`); Literata (optical size, italics) only on article routes (`KVB/DESIGN.md:196-198`). No webfont mono: `--font-mono: ui-monospace, "SF Mono", "Cascadia Code", Menlo, monospace` (`KVB/app/globals.css:28`). "No monospace and no uppercase kickers in the reader UI" (`KVB/DESIGN.md:198`). Inter was explicitly rejected as "a generic default" and Golos as denser (`KVBD/KVB-REDESIGN-PLAN.md:496-499`).

**Type roles** (`KVB/tailwind.config.ts:100-115`; `cn()` taught these names `KVB/lib/utils.ts:7-26`):

| Role | Size | LH | Tracking | Weight |
|---|---|---|---|---|
| `display-0` | `clamp(34px, 22.3px + 2.6vw, 60px)` | 1.02 | −0.03em | 700 |
| `display-1` | `clamp(32px, 25.3px + 1.48vw, 44px)` | 1.04 | −0.028em | 700 |
| `display-2` | `clamp(30px, 24.7px + 1.48vw, 46px)` | 1.06 | −0.025em | 700 |
| `headline-1` | `clamp(24px, 21.3px + .74vw, 32px)` | 1.1 | −0.02em | 700 |
| `headline-2` | `clamp(20px, 18.7px + .37vw, 24px)` | 1.18 | −0.012em | 650 |
| `headline-3` | `clamp(17px, 16.3px + .19vw, 19px)` | 1.28 | −0.006em | 600 |
| `deck` | same clamp as headline-3 | 1.45 | 0 | 400 |
| `ui` | 16px | 1.5 | 0 | 400 |
| `button` | 15px | 1 | 0 | 600 |
| `small` | 14px | 1.4 | 0 | 500 |
| `meta` (tabular) | 13.5px | 1.4 | 0 | 500 |
| `badge` | 13px | 1 | 0 | 600 |
| `caption` | 14px | 1.45 | 0 | 400 |

Rules: `balance` on headlines, `pretty` on body, at most three sizes per module (`KVB/DESIGN.md:200`); `.tabular` utility (`KVB/app/globals.css:436-438`).

**Colour tokens** (`KVB/app/globals.css:31-63` light, `:160-183` dark; contrast table `KVBD/KVB-REDESIGN-PLAN.md:433-452`):

| Token | Light | Dark | Role |
|---|---|---|---|
| `ground` | `#f1f3f8` | `#0a0c11` | page; never pure white |
| `island` | `#ffffff` | `#151821` | primary surface (vs ground 1.11 / 1.10) |
| `inset` (`bg-well`) | `#eef1f8` | `#1e2230` | rows, callouts, filled fields — **lighter** than island in dark |
| `inset-strong` (`bg-well-strong`) | `#e4e8f2` | `#262b3a` | hover/pressed rows and chips |
| `ink` / `ink-2` / `ink-3` | `#0d1018` / `#3d4352` / `#5e6577` | `#f2f4f8` / `#c3c8d4` / `#9ca3b4` | text; ink-3 is metadata at 5.8:1 |
| `accent` (fill) | `#304ffe` | `#304ffe` | buttons, signal; white on it 5.71:1 |
| `accent-ink` (text-safe) | `#304ffe` | `#8c9cff` | links, focus, current |
| `accent-wash` / `-wash-ink` | `#e8ecff` / `#2338d6` | `#1c2350` / `#b8c2ff` | tint buttons, "our newsroom" |
| `live` / `live-ink` / `live-wash` | `#e5252a` / `#c8161c` / `#fde8e8` | `#e5252a` / `#ff6e73` / `#3a1517` | live badge, live times only |
| `success` / `-wash` | `#0b7a45` / `#e2f5eb` | `#4fd199` / `#10291f` | confirmed |
| `warn` / `-wash` | `#8a5300` / `#fff2d9` | `#f2b654` / `#2e2410` | corrections, **form errors** |
| `ink-island` | `#0d1018` | `#151821` | footer, selected segment |
| `signal` | `= accent` | `= accent` | becomes `live` under `[data-event="major"]` (`:187-190`) |

Brand primitives `--kvb-blue #304ffe`, `--kvb-red #e5252a` from the logo files (`:31-32`); `accent-color` and `caret-color` follow the blue (`:225-226`).

**Accent rules** (named): *Red Means Something*, *One Signal* (at most one saturated island in view; the developing island hides during a major event, `globals.css:194-196`), *Swap, Not Paint* (major-event mode changes two tokens) (`KVB/DESIGN.md:188-192`). Errors are amber, never red (`:184`). No per-category colours (`KVBD/KVB-REDESIGN-PLAN.md:455`). Market direction is ▲/▼ and sign, never red/green (`KVBD/SIGNALS.md:92-93`; `KVB/components/signals/MarketStrip.tsx:47-49`). Tiles on the signal island use `bg-black/[.12]`, which "darkens and never lightens" to keep white text contrast (`KVB/DESIGN.md:182`).

**Radii and squircles.** Base tokens `--r-xs 8 · --r-s 12 · --r-m 16 · --r-l 24 · --r-xl 32 · --r-sheet 28`, `--r-island` = 24 below 768px, 32 from 768px (`KVB/app/globals.css:65-72`, `:132-136`). Squircle implementation, the only one in the family:

1. Two registered, **non-inheriting** multipliers `@property --corner-k` / `--corner-k-sm`, initial 1 (`globals.css:12-23`), so a child never inherits its parent's compensation.
2. Opt-in class, only where supported: `@supports (corner-shape: squircle) { .corner-squircle { corner-shape: squircle; --corner-k: 1.6; --corner-k-sm: 1.35 } }` (`globals.css:198-206`).
3. Every radius utility is `calc(var(--r-*) * var(--corner-k))`; `xs`/`s` use the smaller `--corner-k-sm` (`KVB/tailwind.config.ts:10-12`, `:116-126`), named `rounded-sq-*` and `rounded-island` because Tailwind owns `rounded-r-*` (`:117`).
4. Concentric media radius: `calc((var(--r-island) - var(--island-pad, 8px)) * var(--corner-k))` (`globals.css:341-343`); rule `r_inner = max(r-xs, r_outer − padding)` (`KVBD/KVB-REDESIGN-PLAN.md:530`).
5. Fallback is ordinary round corners (Safari, Firefox); "No SVG masks, `clip-path` geometry or JS (Houdini, figma-squircle). Those clip focus rings and borders" (`KVBD/KVB-REDESIGN-PLAN.md:531-536`). Effective squircle radii: 8→11, 12→16, 16→26, 24→38, 32→51 (`:520-527`). The focus outline follows the squircle (`:467`).
6. Pills stay round: chips, badges, segmented control, search, icon buttons (`KVB/DESIGN.md:214`; `KVBD/KVB-REDESIGN-PLAN.md:528`).

**Spacing and layout.** 4pt scale `0, 2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 80, 96` (`KVBD/KVB-REDESIGN-PLAN.md:505`); page padding 12/16/24/32 (`KVB/app/globals.css:322-337`); island gap 8/12/16 (`KVBD/KVB-REDESIGN-PLAN.md:509`); island padding `p-4 md:p-5 xl:p-6`, media `p-2 md:p-3`, roomy `p-5 md:p-10 xl:p-14` (`KVB/components/system/Island.tsx:12-18`); content max 1376px, measure 680px (`KVB/tailwind.config.ts:131-136`).

**Borders.** None for separation. Lines are allowed only for data tables, focus rings, transparent borders for forced-colors mode, and author `<hr>` (`KVBD/KVB-REDESIGN-PLAN.md:544-548`); islands carry `border border-transparent` and `data-surface` (`KVB/components/system/Island.tsx:28-31`). Article tables use zebra `inset` rows instead of rules (`KVB/app/globals.css:771-773`).

**Shadows.** One token, `shadow-float: 0 12px 32px -12px rgb(13 16 24 / 0.28)`, "only for things that genuinely float" (`KVB/tailwind.config.ts:127-130`): Explore panel, new-signals pill, skip link (`KVB/components/navigation/ExplorePanel.tsx:18`; `KVB/components/signals/NewSignals.tsx:69`; `KVB/app/layout.tsx:101-103`). Plan adds a dark-mode ring `0 0 0 1px rgb(255 255 255 / .06)` (`KVBD/KVB-REDESIGN-PLAN.md:551`) that is **not** implemented in the config. Sheets and dialogs use a scrim, not a shadow (`:553`). An e2e test asserts no surface casts a shadow (`KVB/DESIGN.md:218`).

**Motion** (`KVB/app/globals.css:74-92`, documented `KVBD/NAVIGATION-AND-MOTION.md:220-235`): `--motion-press 90ms`, `--motion-fast 140ms` (hover colour), `--motion-base 200ms` (state), `--motion-panel 280ms` in / `--motion-panel-exit 180ms` out, `--motion-page 200ms` (fade only), `--motion-media 420ms`; `--ease-standard cubic-bezier(.2,0,0,1)`, `--ease-enter cubic-bezier(.05,.7,.1,1)`, `--ease-exit cubic-bezier(.3,0,1,1)`; distances 2/4/8/16px; `--scale-press .98`; `--stagger 24ms`. Mapped to Tailwind `duration-press|quick|base|panel|media`, `ease-standard|enter|exit` (`KVB/tailwind.config.ts:144-155`). Rules: transform/opacity only, no springs, exits faster than entries, nothing loops (live dot pulses three times, `globals.css:371-382`) (`KVBD/NAVIGATION-AND-MOTION.md:237-243`). **Reduced motion is a token swap**: distances and scales to identity, durations to 0 or 80–120ms crossfades (`KVB/app/globals.css:141-158`), so components need no reduced-motion code (`KVB/DESIGN.md:241`).

**Focus.** `:focus-visible { outline: 2px solid accent-ink; outline-offset: 2px }`, white on the signal island (`KVB/app/globals.css:243-251`); whole-card ring via `.story-link:has(.stretched-link:focus-visible)` (`:353-360`). Hover styles only where hover exists (`future.hoverOnlyWhenSupported`, `KVB/tailwind.config.ts:20-21`).

**Density.** Reader register: 44px primary controls, 36px small (`KVB/components/system/ButtonLink.tsx:19-22`), 16px UI text. The Signals "wire" is the dense exception (section 2).

### 1.3 NoirSystems site (brand)

**Concept.** "Monochrome protocol field": a continuous black or off-white field; structure from type, alignment and spacing; a surface only when it holds an interaction (`NS/design.md:25-31`). Product surfaces: "Borders are the exception… Space does the grouping. One light accent, reserved for the single primary action… State is carried by an icon plus a word" (`NS/app/noir.css:1-11`).

**Typefaces.** Geist Sans and Geist Mono, `next/font/google`, latin (`NS/app/layout.tsx:6-14`; mapped `NS/app/globals.css:38-39`). Mono only for identifiers, callbacks, subjects, constants, commands and code; labels sentence case (`NS/design.md:47-53`).

**Colour tokens** (`NS/app/globals.css:6-17`; `NS/design.md:60-71`): `--noir-black #070708`, `--noir-black-raised #111114`, `--noir-graphite #1b1b20`, `--noir-line #303036`, `--noir-white #f5f5f2`, `--noir-white-muted #b8b8b3`, `--noir-paper #f2f2ef`, `--noir-paper-raised #ffffff`, `--noir-ink #090909`, `--noir-ink-muted #5f5f5a`. Product surface set (`NS/app/noir.css:19-38`): `--ground #0a0a0c`, `--paper #121217`, `--raised #1b1b21`, `--sunken #0e0e12`, `--wash rgb(244 244 242 / .08)`, `--wash-strong / .14`, `--line rgb(244 244 242 / .14)`, `--line-strong / .24`, ink `#f4f4f2 / #babab8 / #8b8b91 / #85858b`, `--accent #f4f4f2`. Console (`NS/app/console/console-workspace.css:1-10`): `#08080a` ground, `#0d0d10` sidebar, `#121216` raised, lines `#29292f` / `#3b3b42`. The wallpaper pack reuses `#070708 #111114 #303036 #f5f5f2 #b8b8b3` (`/Volumes/NoirSystems/Wallpapers/review/index.html:8`).

**Accent rules.** "White or black inversion is the only brand accent. Status never depends on hue. Every status carries an icon and an explicit word" (`NS/design.md:73-74`); enforced by `Marker`, which always ships a default icon per state (`NS/components/noir/Marker.tsx:4-19`); invalid fields use a dashed border, not red (`NS/app/console/console-workspace.css:659-662`).

**Radii.** "Contextual: 8px fields, 12px grouped controls, 18px overlays", buttons keep a pill (`NS/design.md:88-90`); tokens `--radius-shell 18 · --radius-card 12 · --radius-inner 8 · --radius-pill 999` (`NS/app/noir.css:49-52`; `NS/app/globals.css:28-31`). No squircles.

**Spacing.** `4, 8, 12, 16, 24, 32, 48, 72, 96, 128` (`NS/design.md:81`).

**Borders and elevation.** Hairlines are used more than in Flokzy/KVB: "A hairline on near-black has to be brighter… or the structure disappears" (`NS/app/noir.css:29-32`); "lift comes from surface tone plus a hairline, with only a whisper of shadow" (`:40-45`: `--lift-1 0 1px 2px rgb(0 0 0/.5)`, `--lift-2 0 2px 4px rgb(0 0 0/.5), 0 12px 32px rgb(0 0 0/.45)`). Shadows only for overlays that physically float (`NS/design.md:91-92`). The console uses a ruled sidebar and row rules (`NS/app/console/console-workspace.css:49`, `:568-580`).

**Motion.** Hover/press 120–180ms, state 180–260ms, major 420–700ms; opacity plus ≤8px translation; no perpetual animation (`NS/design.md:96-110`). Easing in practice `cubic-bezier(0.16, 1, 0.3, 1)` (buttons `NS/components/noir/button.css:31-34`, `:44`; nav `NS/components/noir/nav/nav.css:81-86`). Reduced motion: global 0.01ms clamp (`NS/app/globals.css:169-182`).

**Focus.** `outline: 2px solid currentColor; outline-offset: 4px` (`NS/app/globals.css:89-92`); fields: border to accent + `0 0 0 3px var(--wash)` (`NS/app/noir.css:418-421`).

**Density.** The console is the densest surface in the family: 224px rail (`console-workspace.css:38-42`), 42px nav rows (`:78-93`), table text 0.67–0.76rem (≈11–12px) (`:568-592`), 38px selects and 44px fields with 8px radius (`:188-197`, `:644-653`).

**Logo usage.** Favicon is an "N" stroked in `#F2F2EE` on a `#070707` square with `rx=7` (`NS/public/favicon.svg`); wordmark mark is a 28px mono glyph tile, 8px radius, 1px `--rule-strong` border (`NS/app/globals.css:122-142`).

### 1.4 `libs/xandra-css` (library)

What it is: "a standardization contract… ~85 classes" with zero JS, container-query cards and an audit system for deviations (`XC/README.md:1-5`; class list `XC/src/xandra.css:49-91`). Ships npm `xandra`, the `xcc` compiler and a VS Code extension (`CLAUDE.md:18`).

Tokens: spacing 4–64px (`XC/src/_tokens.css:11-18`), system-ui / ui-monospace fonts (`:24-25`), radii 4/8/12/full (`:53-56`), transitions 100/150/300ms `ease` (`:61-63`), z-index 0/100/200/300/400/9999 (`:76-81`). Dark default theme: base `hsl(240 10% 4%)`, surface `hsl(240 6% 8%)`, elevated `hsl(240 5% 12%)`, accent `hsl(217 91% 60%)` (Tailwind blue-500), focus ring `0 0 0 3px hsl(217 91% 60% / .4)` (`XC/src/_themes.css:10-49`). Also light, a "Night Vision Green" tactical theme and high-contrast (`:54-180`). Density by token override (`.x-dense`, `XC/src/_spacing.css:60-67`).

Does it carry the family look? **No.** No squircles or `corner-shape`; no Inter/Onest/Geist; a blue accent none of the products use; cards are hairline + shadow (`XC/src/_components.css:155-167`) and inputs bordered (`:298-325`), opposite to the tonal-island model; no `prefers-reduced-motion` handling (only `prefers-color-scheme`, `_themes.css:188`). Technically risky next to Tailwind v4: none of its CSS is in a cascade layer (zero `@layer` in `src/` and `dist/`), and its reset sets `* { margin: 0; padding: 0 }` (`XC/src/_reset.css:12-15`); unlayered rules beat Tailwind v4's layered utilities, so `p-4`/`m-2` would stop working. Header says "Version: 2.1.0" (`XC/src/xandra.css:5`) while the package is 2.3.0.

May a product consume it? Yes by rule (`docs/dependency-rules.md:5`, products → libs allowed). It currently has no in-repo consumer (`docs/dependency-rules.md:35`).

---

## 2. Component patterns

| Pattern | Flokzy | KVB News | NoirSystems site |
|---|---|---|---|
| **App shell** | Open canvas; sidebar and content share `--background`; pages compose floating `.island`s (`FZ/src/app/[locale]/app/(dashboard)/layout.tsx:29-48`; `FZ/src/app/globals.css:597-623`). | Sticky masthead on the ground that hides after 64px down and returns after 12px up, never while focus is inside (`KVB/components/navigation/MastheadBar.tsx:15-16`, `:80-88`; `KVB/app/globals.css:500-508`); `.page-frame` (`:322-337`). | Console: 224px ruled rail + main (`NS/app/console/console-workspace.css:38-51`). |
| **Sidebar nav** | Resizable 200–400px, default 256, collapses to 56px, drag-reorder, persisted (`FZ/src/components/dashboard/sidebar.tsx:37-40`, `:370-376`); items `h-9 rounded-md px-3 gap-2.5` with 15px icon, active `bg-accent-3 font-medium` + `accent-11` icon (`:173-196`); group label `text-caption-1 text-muted-foreground/60` (`:275`); 1px resize handle tinted `ring/40` on hover (`:545-553`); mobile: fixed drawer + `bg-black/40` scrim (`:363-374`). | Public site has no sidebar; admin still uses legacy shadcn sidebar (`KVB/app/admin/layout.tsx:24-50`). Nav rows in panels: `corner-squircle rounded-sq-m p-2.5 min-h-11`, hover `bg-well` (`KVB/components/navigation/NavParts.tsx:43-45`); icon on a 40px tinted squircle tile, current = `accent-wash` (`KVB/components/navigation/NavIcon.tsx:44-54`). | Rail buttons 42px, 8px radius, hover `#19191e`, current 610 weight (`console-workspace.css:78-103`). |
| **Top bar** | None globally; each page's header is an island (title, mono team key, count pill, view toggle, group-by select) (`FZ/src/app/[locale]/app/(dashboard)/tasks/page.tsx:467-520`); breadcrumb `text-body-2 text-muted-foreground` (`FZ/src/components/layout/app-breadcrumb.tsx:9-10`). | Primary nav as pills with **one sliding indicator** (`h-10 rounded-full`, `--motion-panel`, `ease-standard`) (`KVB/components/navigation/PrimaryNav.tsx:113-138`); icon buttons 44px round island (`KVB/components/layout/ThemeToggle.tsx:16-18`). | Floating pill bar, 55% surface + `blur(18px)`, 320ms (`NS/components/noir/nav/nav.css:76-86`). |
| **Buttons** | Base UI + cva; variants `default` (indigo), `neutral` (inverted), `outline` (tonal `bg-muted/70`, no border), `secondary`, `ghost`, `destructive` (10% tint), `link`; sizes xs 28 / sm 32 / default 36 / lg 40 + icon sizes; press `translate-y-px` (`FZ/src/components/ui/button.tsx:6-44`). | `primary` (fill, hover 8% darker via `color-mix`), `tint` (wash), `island`, `on-signal`; md 44 / sm 36; `corner-squircle rounded-sq-s` or pill; `.motion-press` scale .98 (`KVB/components/system/ButtonLink.tsx:10-48`; `KVB/app/globals.css:474-481`). | Pill, 1px line, fill wipes in from the left on hover (220ms); sizes 36/44/52; primary is inverted black/white (`NS/components/noir/button.css:18-19`, `:37-45`, `:75-98`). |
| **Inputs / forms** | Filled, borderless: `rounded-md bg-well px-2 py-1.5 text-body-1 focus:ring-2 focus:ring-ring/40` (`FZ/src/app/[locale]/app/(settings)/settings/profile/page.tsx:114`); compact search `h-7 rounded-md bg-[var(--bg-subtle)]` (`FZ/src/components/tasks/task-filter-bar.tsx:338-345`). | Pill search `h-10 rounded-full bg-island pl-10` with a 16px bold glyph (`KVB/components/signals/WireControls.tsx:79-100`); plan: filled fields on `inset`, errors amber + icon + text (`KVBD/KVB-REDESIGN-PLAN.md:437`, `:456`). | Bordered shell 38px, `--ground` fill, 8px radius (`NS/app/noir.css:404-421`); console 44px, invalid = dashed border (`console-workspace.css:644-662`). |
| **Selects / menus / popovers** | Base UI positioned popup, portalled, `z-popover`, surface `rounded-lg bg-popover shadow-xl` (`FZ/src/components/ui/popover.tsx:20-21`, `:186-199`); select trigger `bg-well` or inline ghost, chevron rotates (`FZ/src/components/ui/select.tsx:215-246`); items `rounded-md px-2 py-1.5 text-body-2`, highlight `bg-muted` (`:361-366`); menu motion (`:29-30`). | Explore is a **disclosure**, not an ARIA menu: one island under the masthead, `shadow-float`, page behind dims (`KVB/components/navigation/ExplorePanel.tsx:18`; `KVB/app/globals.css:512-548`); hover intent 220ms open / 260ms close grace / click within 600ms pins (`KVB/components/navigation/ExploreMenu.tsx:23-25`). | Native `<select>` styled 38px, 8px radius (`console-workspace.css:188-205`). |
| **Tooltips** | Native `title` attributes only (e.g. `sidebar.tsx:176`, `:215`); no tooltip component. | Native `title` + `aria-label` on icon buttons (`KVB/components/layout/ThemeToggle.tsx:29-31`); Radix tooltip only in legacy admin `ui/`. | — |
| **Tables / lists** | Task list = island with `divide-y` rows; uppercase `text-body-2` group header with count (`tasks/page.tsx:728-735`); row `flex gap-3 px-3 py-2 text-body-1`, hover/selected = canvas tone, keyboard focus `ring-1 ring-inset` (`FZ/src/components/tasks/task-row.tsx:96-100`); fixed 64px identifier column `text-caption-1 tabular-nums` (`:185`); hover-revealed actions (`:365`). | Signals wire: CSS grid rows `[64px time │ 92px priority │ headline │ ≤168px source]`, `rounded-sq-m`, hover `bg-well`, no hairlines (`KVB/components/signals/SignalRow.tsx:55-60`); critical = 3px accent rule on the left (`:63`); time-first `HH:MM` UTC (`:65-70`). | Ruled tables, 0.67rem `th`, 13px row padding (`console-workspace.css:554-600`). |
| **Cards / islands** | `.island { background: var(--card); border-radius: 20px }`, no border, `shadow-xs` only in light (`FZ/src/app/globals.css:616-623`); `.surface-interactive` hover → `card-hover` (`:627-638`); stat island with 28px tabular figure (`FZ/src/app/[locale]/app/(dashboard)/overview/page.tsx:66-76`). | `Island` / `Inset` / `SignalIsland` primitives; never island-in-island or inset-in-inset (`KVB/components/system/Island.tsx:4-63`). | "Not automatically cards"; surfaces only for interaction (`NS/design.md:82-87`). |
| **Badges / chips** | Count pill `rounded-full bg-muted px-2 py-0.5 text-caption-2 tabular-nums` (`tasks/page.tsx:476`); label chip `rounded-md px-1.5 py-0.5 text-[11px]`, 14% colour tint + solid dot (`FZ/src/components/tasks/label-chip.tsx:25-35`, `:59-64`); status as Linear-style shape glyphs (`FZ/src/components/tasks/task-glyphs.tsx:1-9`, `:56-72`); `kbd` `rounded bg-muted px-1.5 font-mono text-caption-2` (`sidebar.tsx:225-227`). | Badges are 26px pills, `text-badge`, tones `live / type / desk / on-signal` (`KVB/components/system/Badge.tsx:6-22`); chips `h-9 rounded-full bg-well` (`KVB/components/navigation/NavParts.tsx:229`); priority = 3 bars + word, never colour alone (`KVB/components/signals/SignalRow.tsx:14-38`). | `Marker`: pill, wash bg, icon + word; pending = inset ring, denied = inverted (`NS/app/noir.css:333-372`). |
| **Tabs / segmented** | Icon toggle in `rounded-md bg-muted/60 p-0.5`, active = inverted `bg-foreground text-background` (`tasks/page.tsx:484-505`); theme choice as selectable tiles (`FZ/src/app/[locale]/app/(settings)/settings/appearance/page.tsx:34-38`). | `aria-pressed` buttons on a `bg-well` pill track; one `bg-ink` thumb slides (`duration-base`) (`KVB/components/home/RailTabs.tsx:17-46`). | — |
| **Dialogs / sheets** | Framer Motion modal, `bg-black/30 backdrop-blur-[2px]`, `z-modal` (`FZ/src/components/ui/overlay.tsx:60-85`); confirm dialog `rounded-2xl bg-popover shadow-2xl` with tinted icon disc (`FZ/src/components/ui/confirm-dialog.tsx:186-216`); right drawer 620px, `bg-card shadow-2xl`, spring (`overlay.tsx:97-148`). | Radix Dialog bottom sheet, `rounded-t-sq-sheet bg-ground`, floats at 560px from 640px, `bg-black/40` scrim, slide 280/180ms, fade under reduced motion (`KVB/components/navigation/MobileMenu.tsx:34-60`; `KVB/app/globals.css:551-566`). | Dialogs/menus may use a contained 18px surface (`NS/design.md:85`, `:88-90`). |
| **Toasts** | `sonner`, bottom-right, `richColors`, `closeButton`, themed (`FZ/src/components/ui/themed-toaster.tsx:9-14`); `z-toast 700`. | None on the reader site. Closest: "N new signals" pill, polite live region, never auto-inserts content (`KVB/components/signals/NewSignals.tsx:14`, `:47-50`, `:64-74`). | — |
| **Empty states** | Island, 40px muted icon disc, one primary + keyboard hint (`tasks/page.tsx:669-700`). | Hide the block when data is empty (`KVB/DESIGN.md:247`); filter no-match line `role="status"` (`KVB/components/signals/WireControls.tsx:109-113`). | `.noir-empty` centred muted text (`NS/app/noir.css:1503-1508`, unused selector). |
| **Loading / skeletons** | `animate-pulse rounded-md bg-muted/50`, skeletons in final geometry per screen (`FZ/src/components/ui/skeleton.tsx:3-104`). | `SkeletonBlock` `rounded-sq-s bg-well`, static under reduced motion (`KVB/components/system/Skeleton.tsx:4-10`); final-geometry page skeleton (`KVB/app/(site)/loading.tsx:6-31`). | — |
| **Error states** | `rounded-xl bg-[var(--bg-subtle)]`, coral icon + text, ghost Retry (`FZ/src/components/ui/error-state.tsx:18-35`). Red for errors. | Roomy island, plain sentence, primary "Try again", reference digest; no false "team notified" (`KVB/app/(site)/error.tsx:5-21`). Amber, never red. | Dashed border + icon, no hue (`console-workspace.css:659-662`). |
| **Command palette / search** | `cmdk`, `max-w-[640px] rounded-2xl bg-[var(--card)] shadow-lg`, `pt-[12vh]`, blur scrim (`FZ/src/components/tasks/command-palette.tsx:155-173`); uppercase caption group headings (`:182`); items `rounded-md px-2.5 py-2 text-body-2`, selected `bg-subtle`, right-aligned `kbd` (`:564-584`); footer key hints (`:382-398`); ⌘K hint in sidebar (`sidebar.tsx:212-229`); floating bulk bar `rounded-xl bg-popover shadow-xl` (`FZ/src/components/tasks/bulk-bar.tsx:91-93`). | No search until a backend exists (`KVB/DESIGN.md:223`). Keyboard on the wire: `j`/`k` move, `/` focuses filter, `Esc` clears, `m` toggles Major only; ignored while typing (`KVB/components/signals/WireControls.tsx:50-73`); hints rendered as `kbd` (`:102-106`). | — |

---

## 3. Iconography

| | Flokzy | KVB News | NoirSystems site |
|---|---|---|---|
| Library | `lucide-react` (`FZ/components.json:13`; ~100 import sites in `src/`) | Phosphor `@phosphor-icons/react` 2.1.10, SSR entry for zero client JS (`KVBD/KVB-REDESIGN-PLAN.md:559`, `:568`; `KVB/components/navigation/NavIcon.tsx:1-20`). Lucide/Radix icons remain only in legacy admin `ui/`. | Phosphor for interface actions (`NS/design.md:211-212`); `simple-icons` for framework marks |
| Stroke / weight | Lucide default stroke 2 (explicit `strokeWidth={2}` on status glyphs, `FZ/src/components/tasks/task-glyphs.tsx:49`; a handful of 1.5/2.5/3 overrides) | Two weights only: `fill` for identity/state, `bold` for small actions; never thin/light/regular/duotone (`KVBD/KVB-REDESIGN-PLAN.md:561-566`) | Mostly `bold` (58 uses) and `fill` (43), some `regular` (34), one `light` |
| Sizes | 13px (113 uses), 14 (108), 12 (89), 15 for nav (`sidebar.tsx:193`) | 16 meta, 20 controls, 24 nav, 28 in 56px category squircles (`KVBD/KVB-REDESIGN-PLAN.md:573`); `size-5` nav icons (`NavIcon.tsx:40`) | 15–20px, most often 17 and 18 |
| Treatment | Thin, uncoloured except status glyph colours (`task-glyphs.tsx:65-72`) | Filled glyph on a tinted squircle tile when standalone; 8px icon–label gap; icon-only buttons 44×44 with label and tooltip (`KVBD/KVB-REDESIGN-PLAN.md:574-577`; `NavIcon.tsx:44-54`) | Status icon always paired with a word (`NS/components/noir/Marker.tsx:4-19`) |

The KVB plan records that the owner defined NoirSystems "maxi" as "friendly filled iconography instead of thin outline icons" (Flokzy ENG-611) (`KVBD/KVB-REDESIGN-PLAN.md:557`); Flokzy's shipped UI has not adopted that yet.

---

## 4. Family resemblance vs product-specific

### Shared (the family)

1. **Dark near-black field, neutral-to-cool.** Grounds `#070708` (NS), `#0a0a0b` (FZ), `#0a0a0c` (NS product), `#0a0c11` (KVB). First surface `#111114` (NS raised, FZ well), `#121217` (NS paper), `#151821` (KVB island), `#161619` (FZ card). Second surface `#1b1b20`–`#1b1b21` (NS), `#1c1c21`–`#202026` (FZ), `#1e2230` (KVB inset). Ink `#f2f4f8` / `#f4f4f2`–`#f5f5f2` / `#fafafa`; muted `#a1a1aa` / `#b8b8b3` / `#c3c8d4`. Steps between surfaces are small (KVB island vs ground 1.10:1, `KVBD/KVB-REDESIGN-PLAN.md:436`).
2. **Flat; tone instead of borders; shadows only for floating things.** `FZ/src/app/globals.css:67-69`, `:122-123`; `KVB/DESIGN.md:163`, `:169`; `NS/app/noir.css:6`, `NS/design.md:91-92`.
3. **12px control radius, pills for small state.** Flokzy `--radius-md 12` buttons/inputs (`FZ/src/app/globals.css:116`); KVB `--r-s 12` buttons/inputs (`KVB/app/globals.css:67`); NS `--radius-card 12` grouped controls (`NS/app/noir.css:50`). Badges/chips/segmented are full pills in all three. Panels 18–32 (NS 18, FZ 20/24, KVB 24/32).
4. **4px spacing base** (`FZD/DESIGN_OVERHAUL_PLAN.md:113`; `KVBD/KVB-REDESIGN-PLAN.md:505`; `NS/design.md:81`; `XC/src/_tokens.css:11`).
5. **Restraint with colour.** One primary action (`FZD/DESIGN_OVERHAUL_PLAN.md:112`; `NS/design.md:21`, `NS/app/noir.css:7`), one signal zone (`KVB/DESIGN.md:190`); state never by hue alone (`NS/design.md:73-74`; `KVBD/SIGNALS.md:88-91`; Flokzy shape glyphs `FZ/src/components/tasks/task-glyphs.tsx:1-9`).
6. **No decoration.** No gradients, glass, glow, fake chrome or marquees (`FZD/DESIGN_OVERHAUL_PLAN.md:109`; `KVBD/KVB-REDESIGN-PLAN.md:55`; `NS/design.md:259-269`).
7. **Honest data.** Show only what exists; unknown is not "low"; no fake metrics (`KVB/DESIGN.md:165`, `:247`; `KVBD/SIGNALS.md:90-91`; `NS/design.md:216`).
8. **Motion is opacity/transform, short, and off under reduced motion** (`FZ/src/app/globals.css:316-328`; `KVB/app/globals.css:141-158`; `NS/app/globals.css:169-182`).
9. **Sentence-case labels, tabular numerals for data** (`NS/design.md:53`; `KVBD/KVB-REDESIGN-PLAN.md:488`; `FZ` `tabular-nums` throughout, e.g. `task-row.tsx:185`).
10. **Russian product-school lineage**: Gravity UI for Flokzy (`FZ/src/app/globals.css:6`; `FZD/DESIGN_OVERHAUL_PLAN.md:4`), Ozon/T-Bank/Yandex for KVB (`KVBD/KVB-REDESIGN-PLAN.md:48-53`).

### Product-specific

| Axis | Flokzy | KVB News | NoirSystems site |
|---|---|---|---|
| Typeface | Inter + JetBrains Mono | Onest (+ Literata for reading), system mono | Geist + Geist Mono |
| Accent | Indigo `#5661d8` (dark) | KVB blue `#304ffe` fill / `#8c9cff` ink (dark); red = live only | Monochrome inversion |
| Squircles | No | Yes (`corner-shape`, opt-in) | No |
| Icons | Lucide, thin | Phosphor fill + bold | Phosphor bold/fill/regular |
| Inset direction (dark) | Well is **darker** than card (`#111114` < `#161619`) | Inset is **lighter** than island (`#1e2230` > `#151821`) | Sunken `#0e0e12` < paper `#121217`; raised `#1b1b21` |
| Easing | expo-out `(0.16,1,0.3,1)` + springs | `(.2,0,0,1)` / `(.05,.7,.1,1)` / `(.3,0,1,1)`, no springs | expo-out `(0.16,1,0.3,1)` |
| Hairlines | Residual (rows, palette, sidebar) | None except tables/focus | Deliberate (tables, rails) |
| Control height | 28–40px | 36–44px | 36–52px buttons; 38–44px fields |
| Errors | Red (coral) | Amber | Dashed border + icon, no hue |
| Light mode | Canvas `#f4f4f6`, white islands | Cool ground `#f1f3f8`, white islands | Paper `#f2f2ef` |

---

## 5. Recommendations for Lavender

Lavender's brief (dark-first, dense but readable, flat, squircles, restrained accent, minimal borders and shadows) is the intersection of **Flokzy's dark density** and **KVB's shape and motion system**, under the **NoirSystems** status and honesty rules.

### 5.1 Depend on `libs/xandra-css`, or copy tokens?

**Copy the values; do not depend on xandra-css.**

- Allowed but unsuitable: products may import `libs/*` (`docs/dependency-rules.md:5`), but xandra's tokens are not the family's (blue-500 accent, system fonts, bordered and shadowed cards, no squircles, no reduced-motion handling; section 1.4). Its unlayered `* { margin: 0; padding: 0 }` reset would override Tailwind v4's layered utilities if Lavender uses Tailwind.
- **Do not import** CSS or TS from `products/flokzy/**` or the KVB worktree: product→product is forbidden (`docs/dependency-rules.md:6`). Copy values into Lavender's own token file, with a comment on each block citing its source file and line.
- **Do not create a shared token lib yet.** `libs/` is earned when a second product in a different domain already imports the code (`CLAUDE.md:50-52`; `docs/dependency-rules.md:28-31`). If Lavender and a second product later import the same token file, that is the moment to propose something like `libs/noir-tokens`.
- Ideas worth borrowing from xandra without the dependency: density by token override (`.x-dense`, `XC/src/_spacing.css:60-67`) and the `data-ns` deviation audit (`XC/src/_ns-audit.css`). The "Night Vision Green" theme (`XC/src/_themes.css:94-136`) is a possible starting point if Lavender ever needs a night-ops mode, but it is not part of the family look.

### 5.2 Adopt verbatim

| What | From | Why |
|---|---|---|
| Squircle mechanism: `@property --corner-k`/`--corner-k-sm` (non-inheriting), `@supports (corner-shape: squircle) .corner-squircle {…; --corner-k: 1.6; --corner-k-sm: 1.35}`, radii as `calc(var(--r-*) * var(--corner-k))`, opt-in per component, round-corner fallback, no masks/clip-path/JS | `KVB/app/globals.css:12-23`, `:198-206`; `KVB/tailwind.config.ts:10-12`, `:116-126`; `KVBD/KVB-REDESIGN-PLAN.md:531-536` | The only squircle implementation in the family; it keeps focus rings and costs no JS, which matters next to a WebGL canvas. |
| Concentric radius rule `r_inner = max(r-xs, r_outer − padding)` and `.rounded-media` | `KVBD/KVB-REDESIGN-PLAN.md:530`; `KVB/app/globals.css:341-343` | Nested panels, thumbnails and layer swatches stay coherent. |
| Motion tokens by job, three easings, reduced motion as one token swap | `KVB/app/globals.css:74-92`, `:141-158`; rules `KVBD/NAVIGATION-AND-MOTION.md:237-250` | Most rigorous system in the family; one switch covers reduced motion. Prefer it to Flokzy's per-component springs. |
| Z-index ladder (`raised 10 … tooltip 800`) and the `no-raw-z-index` lint | `FZ/src/app/globals.css:131-153`; `FZ/eslint.config.mjs:70`, `:175` | A globe app stacks HUD, drawers, popovers over the canvas; the ladder already encodes "popover above modal". |
| Type-drift lint (`no-design-drift`) | `FZ/eslint.config.mjs:15-34` | Keeps a dense UI on its scale. |
| Focus ring: 2px outline, 2px offset, accent text colour; white/ink on saturated surfaces | `KVB/app/globals.css:243-251` | `outline` follows `corner-shape`; ring-shadow approaches (Flokzy `ring-3`) do not. |
| Status = icon + word; priority = bars + word; unknown is omitted, not "low" | `NS/components/noir/Marker.tsx:4-19`; `KVB/components/signals/SignalRow.tsx:14-38`; `KVBD/SIGNALS.md:88-91` | Map symbology is already colour-heavy; status chrome must not depend on hue. |
| "Announce, don't insert" for live data (pill + polite live region + tab-title count) | `KVB/components/signals/NewSignals.tsx:14-74` | Live aircraft/ship/quake feeds must not shift a list the analyst is reading. |
| As-of and delay honesty on every data strip; hide blocks with no data | `KVB/components/signals/MarketStrip.tsx:24-31`; `KVB/DESIGN.md:247` | Freshness is part of an intelligence product's credibility. |
| Command palette anatomy (cmdk, 640px, grouped results, `kbd` hints, footer key legend) and the ⌘K sidebar hint | `FZ/src/components/tasks/command-palette.tsx:155-173`, `:382-398`, `:564-584`; `FZ/src/components/dashboard/sidebar.tsx:212-229` | Place, layer, entity and action search. |
| List keyboarding: `j`/`k`, `/` to filter, `Esc` to clear, ignored while typing | `KVB/components/signals/WireControls.tsx:14-73` | Dense lists of tracks and events. |
| Tonal islands: `.island` (card tone, no border), `.surface-interactive` (hover = one tone up) | `FZ/src/app/globals.css:616-643` | Matches "flat, minimal borders" exactly. |
| Phosphor icons, `fill` for state/identity, `bold` for actions, SSR import where possible | `KVBD/KVB-REDESIGN-PLAN.md:557-577` | Two of three products use Phosphor; it is the documented owner direction (ENG-611). |

### 5.3 Adapt for a dark, map-centric, data-dense tool

1. **The globe is the ground.** In KVB the ground is a colour; in Lavender it is the Cesium canvas. Islands become panels floating over imagery, so they must be **opaque** tonal surfaces (Flokzy `--card #161619` or KVB dark `island #151821`). Avoid `backdrop-filter` over the canvas: the globe repaints continuously, so a blur re-composites every frame, and the brief is flat anyway. (Flokzy uses `backdrop-blur-[2px]` on scrims, `FZ/src/components/ui/overlay.tsx:65`, and the NS nav uses `blur(18px)`, `NS/components/noir/nav/nav.css:79`; neither sits over WebGL.)
2. **Edge definition over bright imagery.** Use tone plus a near-invisible ring instead of drop shadows: KVB's planned dark float ring `0 0 0 1px rgb(255 255 255 / .06)` (`KVBD/KVB-REDESIGN-PLAN.md:551`) and NS's "tone plus a hairline, with only a whisper of shadow" (`NS/app/noir.css:40-45`). Keep `shadow-float` (`KVB/tailwind.config.ts:129`) for menus and popovers only.
3. **Density from Flokzy, not KVB.** KVB's 16px UI text and 44px controls are reader-scale. Use Flokzy's 14px body and its 11–26px app scale (`FZ/src/app/globals.css:19-37`) and 28/32/36/40px controls (`FZ/src/components/ui/button.tsx:25-37`). Raise targets to 44px only under `pointer: coarse` (KVB's targets, `KVBD/KVB-REDESIGN-PLAN.md:690`).
4. **Radius scale, tightened.** KVB's 24–32px island radii (×1.6 → 38–51px) waste corner space on floating HUD panels. Suggested Lavender base values, all existing family values: controls 8 / 12 (KVB `r-xs`/`r-s`, Flokzy `xs`/`md`), panels 16 (KVB `r-m`, Flokzy `lg`), large floating panels and dialogs 20–24 (Flokzy `xl`/`2xl`). Apply KVB's multipliers unchanged.
5. **Choose one inset direction.** Flokzy darkens the well below the card; KVB lightens the inset above the island (section 4). Over a map, a darker well reads as a recessed input and a lighter row reads as hover/selection. Suggestion: Flokzy's sunken well for fields and trays, and KVB's lighter `inset-strong`-style step only for hover/selected rows.
6. **UI accent ≠ data colours.** Keep one restrained UI accent for focus, the primary action and the current item. Map layers need a separate, validated categorical palette; none exists in the family (Flokzy's five chart hues `FZ/src/app/globals.css:209-213` are not enough), so it is new work. Adapt KVB's *Red Means Something* to Lavender: red only for genuine alerts (KVB `KVB/DESIGN.md:188`); form errors amber (`:184`).
7. **Camera motion honours reduced motion.** Map the motion tokens to camera flights: under `prefers-reduced-motion`, cut instead of fly (duration 0), the same way Flokzy collapses JS animation (`FZ/src/lib/motion.ts:15-19`) and KVB swaps tokens. No springs (`KVBD/NAVIGATION-AND-MOTION.md:238`).
8. **Mono only for values compared character by character**: coordinates, MGRS, ICAO/MMSI, timestamps, IDs (`NS/design.md:48-49`; `NS/app/noir.css:9-10`). Everything else in the UI face with tabular numerals.
9. **Right-rail inspector** for the selected entity: Flokzy's drawer anatomy (`max-w-[620px] bg-card`, `z-drawer`) (`FZ/src/components/ui/overlay.tsx:97-98`), without the spring and without the full-screen scrim, so the map stays interactive.

### 5.4 Starter token sheet (proposal; every value traced)

Dark (default). Values copied, not invented; the right-hand column is the source.

| Lavender role | Value | Source |
|---|---|---|
| ground (app chrome around/behind globe) | `#0a0a0b` | `FZ/src/app/globals.css:161` |
| well (sunken fields, trays) | `#111114` | `FZ/src/app/globals.css:192` |
| island (panels over the map) | `#161619` | `FZ/src/app/globals.css:163` |
| island-hover | `#1c1c21` | `FZ/src/app/globals.css:165` |
| popover / floating | `#1c1c22` | `FZ/src/app/globals.css:166` |
| control (tonal buttons, chips) | `#202026` | `FZ/src/app/globals.css:180-182` |
| hairline (tables, focusable regions only) | `#2a2a31` | `FZ/src/app/globals.css:187` |
| hairline-strong | `#3f3f46` | `FZ/src/app/globals.css:193` |
| ink / ink-2 / ink-3 | `#fafafa` / `#a1a1aa` / `#818191` | `FZ/src/app/globals.css:162`, `:183`, `:195` |
| accent ramp (placeholder until Lavender's hue is chosen) | `#1b1d3a #232858 #3f48a8 #5661d8 #6b75e3 #97a0f0` | `FZ/src/app/globals.css:170-175` |
| success / warning / alert | `#45b75f` / `#f39e1e` / `#ef4544` | `FZ/src/app/globals.css:204-205`, `:197` |
| float edge | `0 0 0 1px rgb(255 255 255 / .06)` | `KVBD/KVB-REDESIGN-PLAN.md:551` |
| float shadow (menus only) | `0 12px 32px -12px rgb(13 16 24 / .28)` | `KVB/tailwind.config.ts:129` |
| radii base | 8 · 12 · 16 · 20 · 24 (× `--corner-k`) | `KVB/app/globals.css:66-68`; `FZ/src/app/globals.css:118-119` |
| motion | press 90 · fast 140 · base 200 · panel 280/180 · page 200 · media 420ms; easings `(.2,0,0,1)` `(.05,.7,.1,1)` `(.3,0,1,1)` | `KVB/app/globals.css:76-92` |
| type | caption-2 11/15 … header-1 26/32 | `FZ/src/app/globals.css:21-37` |
| z-index | raised 10 · sticky 100 · widget 200 · nav 300 · drawer 400 · modal 500 · popover 600 · toast 700 · tooltip 800 | `FZ/src/app/globals.css:145-153` |

If Lavender uses Tailwind v4, map these with `@theme inline` as Flokzy does (`FZ/src/app/globals.css:11-154`); v4 opacity modifiers work on hex tokens (Flokzy uses `bg-muted/60`, `ring-ring/50`), so KVB's RGB-channel workaround (`KVB/tailwind.config.ts:8`) is only needed on Tailwind 3. The tokens are plain custom properties, so they also work without Tailwind (the `legacy/` app is a Vite build without it).

### 5.5 Owner decisions (cannot be settled from the repo)

1. **Accent hue.** The name suggests lavender; the nearest existing value is Flokzy `accent-11 #97a0f0` / `accent-9 #5661d8`. Choosing a distinct Lavender hue means building a new 6-step ramp with measured contrast (the method in `KVBD/KVB-REDESIGN-PLAN.md:433-452`).
2. **Typeface.** Dense operational surfaces use Inter (Flokzy) or Geist (NS console, `NS/design.md:159-163`); Onest is KVB's reader face, chosen over Inter for "Maxi" warmth (`KVBD/KVB-REDESIGN-PLAN.md:496-499`). For an intelligence console, **Geist Sans + Geist Mono** aligns with the company brand and its densest existing surface; Inter + JetBrains Mono is the proven alternative.
3. **Light mode.** The brief is dark-first; if light is needed, Flokzy's light block (`FZ/src/app/globals.css:226-297`) is the dense-app reference.
4. **Filled vs outline icons at 12–14px.** KVB's "fill + bold" rule was designed at 16–28px; confirm it holds in dense rows.

### 5.6 Don'ts (from the family's own rules)

- No glass, gradients, glows, 3D tilt, `hover:scale` (`FZD/DESIGN_OVERHAUL_PLAN.md:109`; `NS/design.md:265`).
- No island inside an island, no inset inside an inset (`KVB/components/system/Island.tsx:4-8`).
- No hue-only status; no red except alerts; no invented data or counts (`NS/design.md:73-74`, `:216`; `KVB/DESIGN.md:188`, `:247`).
- No `corner-shape` on `*`; opt in per component so third-party embeds (including Cesium widgets) are untouched (`KVBD/KVB-REDESIGN-PLAN.md:532`).
- No perpetual animation; a live indicator pulses a few times and holds (`KVB/app/globals.css:377-378`; `NS/design.md:106-107`).

---

## 6. Feel, in words

**Flokzy.** A near-black, Linear-adjacent developer tool: the sidebar shares one open canvas, and content sits on borderless charcoal islands with 20px corners that separate by tone. Type is small and tight (Inter at 14px, slight negative tracking, alternate glyphs), icons are thin 12–15px Lucide strokes, and colour is almost absent except Flokzy Indigo on the one primary action and the active nav item, with status carried by shape glyphs. Motion is quick (about 150ms, expo-out) with springy drawers, and everything is reachable by keyboard (⌘K palette, `C` to create). It reads dense, serious and quiet, with a few leftover hairlines between rows and in the palette chrome.

**KVB News.** Calm, product-grade news on a cool tinted ground: flat white (dark `#151821`) squircle islands, tinted insets inside them, no shadows and no hairlines, with large 24–32px corners that become true squircles in Chromium. Onest at substantial sizes, filled Phosphor glyphs and 44px pill and squircle controls give it the confident "Maxi" weight of Russian consumer products, while KVB blue is held to actions and a single saturated signal island and red only ever means live. Motion is token-driven and restrained (90–420ms, Material-style easings, nothing loops). The Signals wire shows the same system can go dense: time-first grid rows, priority bars plus words, `j`/`k` keyboarding.

**NoirSystems site.** A monochrome protocol field: a near-black (`#070708`) or off-white page, Geist Sans with Geist Mono reserved for identifiers, and no hue at all. Inversion is the only accent and every state is an icon plus a word. Structure comes from alignment, spacing and deliberate hairline rules, with pill buttons that fill on hover and a floating blurred pill navigation. The console is the most compact surface in the family (11–12px table text, 8px bordered fields). It feels authored, technical and severe rather than friendly.

**xandra-css** (for completeness). A generic, framework-neutral token and class contract (zinc-style darks, blue-500 accent, bordered cards). Useful ideas, but not the family look.
