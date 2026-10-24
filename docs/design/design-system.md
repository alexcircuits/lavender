# Lavender design system

- Status: v1, 2026-10-11
- Sources: `docs/audit/design-references.md` (every inherited value traced to Flokzy, KVB News or the NoirSystems site), `docs/audit/upstream-ui-and-systems.md` (what Lavender removes)
- Implementation: `apps/web/src/design/tokens.css` and `apps/web/src/design/primitives/*`

## 1. Principles

1. **The globe is the ground.** Chrome sits on the globe as opaque tonal
   islands. Nothing is glass, nothing glows, nothing blurs the canvas.
2. **One accent, one meaning.** Lavender marks *your attention*: the primary
   action, the current item, the focused control and the selected entity on the
   globe. It is never used for data.
3. **Data colours are a separate, validated system** (§4). Status is never hue
   alone: it is always icon + word.
4. **Dense, not cramped.** 14 px body, 32 px controls, 4 px grid; whitespace
   separates groups, not items.
5. **Honest by default.** Every number carries its source and age nearby. Unknown
   is shown as unknown, never as zero or "low".
6. **Motion explains change.** Short, transform/opacity-only and token-driven;
   nothing loops; reduced motion is a single token swap.
7. **Squircles where shape is identity, round elsewhere.** Opt-in
   `corner-shape: squircle` on islands and controls, with plain rounded corners
   as the fallback. Never applied globally (Cesium's credit DOM stays untouched).

What Lavender explicitly does **not** do (carried over from the family's rules
and the upstream audit): gradients, glass, glows, bloom, scanlines, CRT/NVG/FLIR
looks, fake readouts, classification banners, decorative brackets, ALL-CAPS
labels, typewriter text, perpetual animation, hover-scale.

## 2. Typography

| Role | Face | Notes |
|---|---|---|
| Interface | **Geist** (variable, 400/500/600) | Company-brand face (NoirSystems site); designed for dense UI |
| Identifiers & values compared character by character | **Geist Mono** (400/500) | Coordinates, MGRS, ICAO hex, MMSI, NORAD IDs, timestamps, keys. Nothing else is mono |

Self-hosted (`@fontsource-variable/geist`, `…/geist-mono`), `font-display: swap`,
subset to Latin + Latin Extended + Cyrillic. Numbers in the interface face use
`font-variant-numeric: tabular-nums` wherever values update or align.

Scale (Flokzy's dense app scale, renamed by job):

| Token | Size / line | Weight | Use |
|---|---|---|---|
| `--type-micro` | 11 / 14 | 500 | Badge text, axis ticks, legend keys |
| `--type-caption` | 12 / 16 | 400 | Secondary metadata, status bar |
| `--type-body-sm` | 13 / 18 | 400 | Dense lists, table cells |
| `--type-body` | 14 / 20 | 400 | Default UI text |
| `--type-label` | 13 / 18 | 500 | Field labels, row titles |
| `--type-title-sm` | 15 / 20 | 600 | Panel titles |
| `--type-title` | 18 / 24 | 600 | Inspector entity name |
| `--type-display` | 26 / 32 | 600 | Empty-state and onboarding headlines only |

Tracking: −0.006 em at 13–15 px, −0.012 em at 18 px and above, 0 for mono.
Sentence case everywhere. Line length in prose surfaces (help, notices) ≤ 72 ch.

## 3. Colour

### 3.1 Surfaces and ink (dark, default)

| Token | Value | Use | Source |
|---|---|---|---|
| `--ground` | `#0b0c10` | Behind the globe; status bar | noir-design dark ground |
| `--well` | `#1e2027` | Sunken fields, trays, search input | noir-design dark inset |
| `--island` | `#16171d` | Panels over the globe | noir-design dark surface |
| `--island-hover` | `#23242c` | Hovered row / interactive surface | Derived between the family inset and deep-inset steps |
| `--popover` | `#1f1e2a` | Menus, popovers, palette | noir-design deep-surface |
| `--control` | `#262531` | Tonal buttons, chips, segmented controls | Dark of the family deep-inset step, so `--alert` text passes AA on it |
| `--hairline` | `#2b2a39` | Table rules and focusable regions only | noir-design deep-inset |
| `--hairline-strong` | `#3a3847` | Field outlines on hover | noir-design bezel |
| `--ink` | `#f2f3f6` | Primary text (16.1:1 on island) | noir-design dark text |
| `--ink-2` | `#a3a8b4` | Secondary text (7.5:1) | noir-design dark text-2 |
| `--ink-3` | `#8f96a5` | Tertiary text, disabled glyphs (6.0:1 on island, ≥ 5.0:1 on every surface) | Family grey lifted so it passes AA on popover and control |

A light theme is not part of v1. The tokens are role-named so it can be added
as one block later (reference: Flokzy's light block).

### 3.2 Chrome accent (NoirSystems cobalt, measured)

| Token | Value | Contrast | Use |
|---|---|---|---|
| `--accent-ink` | `#9db1ff` | 8.7:1 on island, 9.5:1 on ground | Accent text, focus ring, **selection halo on the globe** |
| `--accent` | `#1e3fd8` | white text 7.6:1 | Primary action fill, active nav indicator (noir-design cobalt) |
| `--accent-hover` | `#3553e3` | white text 6.0:1 | Primary hover |
| `--accent-press` | `#1a36bd` | white text 9.1:1 | Primary pressed |
| `--accent-subtle` | `#1b2852` | | Selected row background (noir-design dark mist) |
| `--accent-muted` | `#223069` | | Selected chip, current nav item background |

### 3.3 Status (reserved, icon + word always)

| Token | Value | Meaning in Lavender |
|---|---|---|
| `--good` | `#4cc38a` | Live, healthy (noir-design dark success) |
| `--warning` | `#f0a33a` | Stale, degraded, polling fallback, recoverable error (noir-design dark warning) |
| `--alert` | `#f4636a` | Source unavailable, a failure that needs action; text and glyphs (≥ 4.9:1 on every surface). Red never decorates |
| `--alert-fill` | `#c62f2f` | Filled destructive buttons in confirm dialogs (white text 5.46:1) |
| `--neutral-state` | `--ink-3` | Unconfigured, disabled by licence policy, off |

## 4. Map symbology (validated)

Validated with the data-viz palette validator (OKLab ΔE, CVD simulation) against
`--island` and `--ground`, with **all pairs** in play, because any layers can be
on screen together.

| Family | Colour | Layers | Secondary encoding (glyph) |
|---|---|---|---|
| Air | `#3987e5` | Aircraft (civil and military) | Plane silhouette rotated to track; military is an outlined variant |
| Hazards | `#d95926` + ramp | Earthquakes, fires, cyclones, perimeters | Quake = ring sized by magnitude; fire = filled cell; cyclone = spiral + track |
| Maritime | `#199e70` | Vessels | Hull chevron rotated to course; at rest = dot |
| Space | `#d6d3e6` (neutral, 12.3:1) | Satellites, orbits | Small diamond; orbit = 1 px line at 40 % |
| Context | `#9a98a8` (neutral) | Infrastructure, cables | Square / line |
| Boundaries | `#e2e8f0` international, `#f59e0b` disputed, `#ec4899` breakaway, `#38bdf8` maritime | Natural Earth admin-0 lines, by class | Line style carries class: solid / long dash / wide solid / short dash |
| Geofences | `#fde047` | Named regions drawn by the analyst | Outlined polygon ring |
| Selection | `--accent-ink` `#9db1ff` | Any selected or tracked entity | 2 px halo + 1 px `--ground` separation ring |

Results: all three hues pass all-pairs (worst CVD ΔE 9.4, worst normal-vision ΔE
20.9, all ≥ 3:1 contrast) on both surfaces. A fourth hue fails all-pairs, so new
layers join an existing family, distinguished by glyph and legend. This is the
designed limit, not an accident.

**Hazard magnitude ramp** (ordinal, one hue, validated): `#8f3c19 #b9471d #d95926 #ef7a45 #f8a57a`
(low → high; brighter means more severe on dark imagery).

Every point symbol carries a 1 px `--ground` outline so it separates from bright
imagery. Labels on the globe use `--ink` with a 2 px `--ground` halo, Geist
500 12 px, and are budgeted by the label arbiter (no label soup).

## 5. Geometry

### 5.1 Radii and squircles (KVB mechanism, Lavender scale)

```css
@property --corner-k { syntax: '<number>'; inherits: false; initial-value: 1; }
@property --corner-k-sm { syntax: '<number>'; inherits: false; initial-value: 1; }
@supports (corner-shape: squircle) {
  .sq { corner-shape: squircle; --corner-k: 1.6; --corner-k-sm: 1.35; }
}
```

| Token | Base | With squircle | Use |
|---|---|---|---|
| `--r-xs` | 6 | 8 (×1.35) | Badges' inner elements, swatches |
| `--r-sm` | 8 | 11 (×1.35) | Small controls (28 px), menu items |
| `--r-md` | 12 | 19 (×1.6) | Buttons, fields, chips |
| `--r-lg` | 16 | 26 (×1.6) | Islands (panels) |
| `--r-xl` | 20 | 32 (×1.6) | Dialogs, command palette |
| `--r-pill` | 999 | — | Status pills, toggles |

The concentric rule is `r_inner = max(--r-xs, r_outer − padding)`. There is no
island inside an island, and no inset inside an inset.

### 5.2 Spacing

A 4 px grid: `--s-0_5: 2`, `--s-1: 4`, `--s-1_5: 6`, `--s-2: 8`, `--s-3: 12`,
`--s-4: 16`, `--s-5: 20`, `--s-6: 24`, `--s-8: 32`, `--s-10: 40`, `--s-12: 48`.
Island padding is 12 (dense) or 16. Gutters between globe edge and islands are 8.

### 5.3 Control sizes

28 (compact: rows, toolbars), **32 (default)**, 36 (prominent), 40 (dialog
primary). Under `@media (pointer: coarse)` the minimum is 44.

### 5.4 Elevation

Elevation is carried by tone, not by shadow: ground → well → island → popover.
Floating surfaces over imagery add `--float-edge: 0 0 0 1px rgb(255 255 255 / .06)`.
Only menus, popovers and the palette add `--float-shadow: 0 12px 32px -12px rgb(0 0 0 / .5)`
(the family's float shadow, deepened for dark imagery). There is no
`backdrop-filter` over the canvas.

### 5.5 Z-index ladder (Flokzy)

`raised 10 · sticky 100 · widget 200 · nav 300 · drawer 400 · modal 500 · popover 600 · toast 700 · tooltip 800`.
Raw z-index values are forbidden in component CSS (lint).

## 6. Layout

```
┌────┬──────────────────────────────────────────────────────────┬──────────────┐
│    │  ┌──────────────┐  ┌──────────────────────┐   ┌────────┐ │              │
│ N  │  │ Layers       │  │ ⌕ Search places, …⌘K │   │ Feeds ●│ │  Inspector   │
│ a  │  │ explorer     │  └──────────────────────┘   └────────┘ │  (on select) │
│ v  │  │ (island,     │                                        │  360 px      │
│    │  │  320 px)     │              G L O B E                 │  island      │
│ r  │  │              │         (full-bleed canvas)            │              │
│ a  │  │              │                                  ┌───┐ │              │
│ i  │  │              │                                  │ + │ │              │
│ l  │  └──────────────┘                                  │ − │ │              │
│    │                                                    │ ⌖ │ │              │
│56px│  [ timeline dock — reserved, collapsed in v1 ]     └───┘ │              │
├────┴──────────────────────────────────────────────────────────┴──────────────┤
│ ● Connected · 4 feeds live   │ 60.1699° N 24.9384° E · 35VLG 85…   │ 14:02:11 UTC │  ← status bar 28 px
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Navigation rail** (56 px, `--ground`): workspace switcher (Globe; later
  Investigations and Reports), Layers, Sources, and Settings at the bottom. It
  expands to 200 px with labels on hover-intent or `⌘\`. The current item uses
  the `--accent-muted` background plus a 2 px accent bar.
- **Layer explorer** (left island, 320 px, resizable 280–440): grouped by family
  (Air, Maritime, Space, Hazards, Weather, Infrastructure, Mobility, Sensors).
  Each row has a family swatch with glyph, name, toggle, count, freshness and a
  disclosure for options.
- **Search** (top, 420 px well): trigger and inline results; `⌘K` opens the full
  command palette.
- **Feed status** (top-right chip): aggregate health. It opens the Sources
  panel.
- **Inspector** (right island, 360 px): appears on selection without moving the
  globe camera. Shows the header (glyph, name/callsign, type), facts (mono
  values), provenance row (source, observed, fetched, age), and actions (Track,
  Follow, Copy coordinates, Open source record).
- **Map controls** (bottom-right vertical cluster of 32 px squircle buttons):
  zoom in/out, north-up/compass, 2D/3D, home, imagery picker.
- **Status bar** (28 px, `--ground`): connection, live feed count, cursor
  position (decimal + MGRS, mono), camera altitude, UTC clock.
- **Timeline dock**: reserved space above the status bar, hidden in v1. Its
  layout slot exists so timeline playback arrives without re-layout.

Alignment: text is left-aligned; numbers in tables are right-aligned; the
inspector's facts form a two-column label/value grid.

## 7. Components

Every component is a React primitive in `src/design/primitives` with CSS
Modules. States are expressed through `data-*` attributes (`data-state`,
`data-selected`, `data-disabled`).

| Component | Spec |
|---|---|
| **Button** | Variants: `primary` (accent fill, one per surface), `secondary` (`--control`), `ghost` (transparent, hover `--island-hover`), `danger` (text `--alert` on `--control`; filled with `--alert-fill` only in confirm dialogs). Sizes 28/32/36. Icon + label, or icon-only with `aria-label` and tooltip. Press: `scale(.98)` 90 ms |
| **IconButton** | Square, same sizes; squircle `--r-md`; tooltip after 500 ms hover or on focus |
| **Toggle (switch)** | 28×16 pill; on = `--accent`; announces "on/off" |
| **SegmentedControl** | Pill track in `--well`, selected segment `--control` + `--ink` |
| **TextField / SearchField** | `--well` fill, no border; hover `--hairline-strong` inset ring; focus ring. Leading icon, clear button, `kbd` hint |
| **Select / Menu** | Base UI; popover surface `--popover`, `--float-edge` + `--float-shadow`, `--r-lg`; items 32 px, `--r-sm`, keyboard type-ahead; checkmark on selected |
| **Popover** | Same surface; arrow omitted; 8 px offset; closes on `Esc` and outside click; focus returns to trigger |
| **Tooltip** | `--popover`, 12 px text, max 280 px; content is a label or a key hint, never essential information |
| **Panel (island)** | `--island`, `--r-lg` squircle, padding 12/16, header 40 px with title + actions; body scrolls; resizable edge has a 4 px hit area and a visible hairline on hover |
| **ListRow** | 32 px (dense 28); hover `--island-hover`; selected `--accent-subtle` + `--ink`; `j`/`k`/arrows; trailing meta in `--ink-2` tabular |
| **Table** | Sticky header in `--type-micro` `--ink-2` sentence case; rows 32 px; hairlines between rows only in tables; numbers right-aligned tabular; mono for IDs; virtualised past 200 rows; column sort with `aria-sort` |
| **Badge / StatusPill** | Pill; icon + word (`● Live`, `◐ Stale 3 min`, `▲ Degraded`, `■ Unavailable`, `○ Not configured`); colour from status tokens |
| **Freshness meter** | 24×4 px bar in layer rows: fill = age ÷ cadence (good → warning past 2× cadence); tooltip states the exact age. This is Lavender's signature data element |
| **Kbd** | Mono 11 px on `--control`, `--r-xs` |
| **Command palette** | 640 px, `--r-xl`, top offset 12 vh, groups (Places, Live entities, Layers, Actions), footer key legend; the same registry powers shortcuts |
| **Toast** | Bottom-centre above the status bar; at most 2 visible; info and success auto-dismiss after 4 s; errors persist with an action |
| **Dialog** | `--r-xl`, scrim `rgb(0 0 0 / .5)` (no blur), focus trapped, `Esc` closes unless destructive |
| **Skeleton** | Static `--island-hover` blocks at the real layout's geometry; no shimmer loop |
| **EmptyState** | Glyph (Phosphor fill, 24 px, `--ink-3`), one sentence saying what to do, one action |
| **ErrorState** | What happened + what to do + action (Retry / Configure); `--alert` or `--warning` glyph per severity |

Icons: Phosphor, **bold** for actions, **fill** for state and identity, 16 px in
32 px controls and 14 px in dense rows.

## 8. Search interfaces

- `/` focuses the search field; `⌘K` / `Ctrl K` opens the palette.
- Coordinates are recognised instantly, without a network call: decimal, DMS
  and MGRS. The parsed point is shown as the first result: "Go to 60.1699° N,
  24.9384° E".
- Results are grouped: Places (geocoder, attribution shown in the group footer),
  Live entities (callsign, ICAO, MMSI, NORAD from active feeds), Layers,
  Actions.
- Each result shows its kind glyph, primary text, secondary context (country,
  type, age) and the action on Enter ("Fly to", "Select", "Turn on").
- No result: "No matches for 'xyz'. Try a place name, coordinates or a callsign."

## 9. Map controls and globe interaction

- The cursor position in the status bar updates on pointer move (throttled to
  animation frames).
- Clicking selects; hover shows a lightweight label (name + age) after 150 ms;
  double-click flies to.
- Tracked entities get the lavender halo and a trail. Follow mode locks the
  camera with smoothing; a visible "Following SAS123 · Esc to stop" chip
  appears at top centre.
- Camera flights run 1.2 s with standard easing. Under reduced motion the camera
  cuts (duration 0).

## 10. Data visualisation in panels

Sparklines and small charts follow the data-viz method: thin marks, recessive
axes, the family hues above, legends for two or more series, and a table view
for anything quantitative. No dual axes. Values in charts use text tokens, never
series colours.

## 11. Loading, empty, error and stale states

| Situation | Treatment |
|---|---|
| App boot | Shell paints immediately with skeleton islands; globe area shows `--ground` with the caption "Loading globe…"; no splash animation |
| Layer turning on | Row shows "Connecting…" and an indeterminate 2 px bar under the row (stops when data arrives; static under reduced motion) |
| First data | Count appears; announcement "Aircraft: 1 248 contacts" (polite, throttled) |
| Stale | Row pill `Stale 4 min`; symbols drawn at 60 % opacity past 3× cadence |
| Source failing | `Unavailable` pill + reason in the row's tooltip and in Sources; last good data kept with a stale treatment, never deleted silently |
| Not configured | Row toggle disabled, `Not configured` + "Add a FIRMS key" link to Sources |
| Disabled by licence policy | Row shows `Licence` pill; Sources explains the policy |
| Search failure | Inline: "Place search is unavailable. Coordinates and live entities still work." |

## 12. Responsive behaviour

| Width | Behaviour |
|---|---|
| ≥ 1440 | Rail + layer explorer + inspector all docked |
| 1024–1439 | Inspector overlays the right edge; opening one side panel collapses the other if they would cover > 60 % of the globe |
| 768–1023 | Rail stays; panels are overlays; search collapses to an icon |
| < 768 | Rail becomes a bottom bar; panels become bottom sheets (snap points 40 % / 90 %); inspector is a sheet; status bar shows connection + UTC only |

## 13. Accessibility

- WCAG 2.2 AA for all chrome; text contrast measured above.
- Focus ring: `outline: 2px solid var(--accent-ink); outline-offset: 2px` (follows `corner-shape`).
- Full keyboard operation: rail (arrow keys), lists (`j`/`k`), palette, inspector actions, and the globe (arrow keys pan, `+`/`−` zoom, `N` north-up, `H` home, `F` follow selection, `Esc` release/close, `?` shortcut sheet).
- Live regions: feed counts are announced politely and throttled to one per 10 s per layer. KVB's announce-don't-insert pattern applies to any list that receives live rows: a "12 new" pill, never a shifting list.
- Every map glyph has a list equivalent (layer row → entity list), so information is never only on the canvas.
- `prefers-reduced-motion`, `prefers-contrast: more` (hairlines → `--hairline-strong`, ink-3 → ink-2), `forced-colors` (system colours, outlines on islands).

## 14. Motion

| Token | Value | Use |
|---|---|---|
| `--motion-press` | 90 ms | Button press scale |
| `--motion-fast` | 140 ms | Hover colour |
| `--motion-base` | 200 ms | State change, toggles, row expand |
| `--motion-panel` | 280 ms in / 180 ms out | Panels, inspector, sheets |
| `--motion-media` | 420 ms | Imagery crossfade |
| `--ease-standard` | `cubic-bezier(.2,0,0,1)` | Most transitions |
| `--ease-enter` | `cubic-bezier(.05,.7,.1,1)` | Entering surfaces |
| `--ease-exit` | `cubic-bezier(.3,0,1,1)` | Leaving surfaces |
| Camera flight | 1 200 ms, standard | Fly-to |

Rules: animate transform and opacity only; exits are faster than entries;
nothing loops (a live dot pulses three times, then holds); no springs. Under
`prefers-reduced-motion: reduce`, distances and scales go to identity,
durations to 0 (80 ms crossfade for panels), and camera flights cut.

## 15. Writing

Sentence case, plain verbs, the user's vocabulary ("Turn on aircraft", "Add a
key", "Track", "Stop following"). Buttons say what happens; the resulting toast
uses the same verb ("Link copied"). Errors state what failed and what to do,
without apology. Units are always shown (km, kn, ft, m/s, UTC). Time is UTC in
the chrome; local time appears alongside in the inspector when relevant.
