# Lavender kanban — orchestrator board

Working board for the orchestrated continuation. One line per card, moved by
hand as waves complete; Flokzy (`task-index.md`) remains the system of record
for the ENG-\* tasks this board references. Update together with
[PROGRESS](../PROGRESS.md). Card numbering: LB-1xx (this board), defined in
[upstream-delta-2026-10.md](upstream-delta-2026-10.md) and
[language-fit.md](language-fit.md).

## Done

- **LB-100 Upstream import.** 1095 commits merged under `legacy/` (`30047df2`);
  `HEAD..upstream/main` empty; legacy byte-identical to `9d3548c2`.
- **LB-101 WIP checkpoint.** Second provider wave + polyglot kernels committed
  (`0eaebbb3`) after fixing contract drift (Freshness union, bigint CCTV
  timestamps, `Located | undefined` + `symbolPx`); web 772/772, `cargo check`
  clean.
- **LB-102 Boundary data relocation.** Derived `boundaries.json` moved to
  `crates/lavender-core/data/natural_earth/` so `legacy/` stays pristine.

## In Progress

- **LB-110 Borders, divisions & region drawing** (owner top priority; feeds
  ENG-966/958). Slice: division hierarchy (admin-0 → admin-1 → counties),
  per-class boundary styling, geofence draw tool on the globe.
- **LB-111 Flight motion model + aircraft simulation** (owner priority).
  Port `legacy/src/layers/flights` + `cockpitMath.js` dead-reckoning into
  lavender-core; synthetic aircraft mode.
- **LB-112 Launch ascent simulation** (owner priority). Trajectory integration
  + time-scrubbable arc from the launches provider.
- **LB-113 In-app kanban mission boards** (owner priority; Gotham-inspired).
- **LB-133 NoirSystems token alignment** in `apps/web/src/design/tokens.css`
  against `libs/noir-design/src/styles/tokens.css` (dark register), keeping
  `tokens.test.ts` contrast pairs green.
- **Research: Palantir Gotham feature set** → feeds LB-113/LB-12x scope
  (report pending; doc will land in `docs/research/`).

## Backlog (ranked)

- **LB-109** Per-provider verification of the landed second wave (fixtures,
  licence lines, renderer journeys) — gates ENG-990/991 completion.
- **LB-114** airQuality provider · **LB-115** GNSS integrity · **LB-116**
  streetLevel remainder (Mapillary/KartaView) · **LB-117** SDR/local receivers
  via Zig native artifact · **LB-118** scenes/evidence tours · **LB-119** maps
  catalog/credits · **LB-120** i18n decision · **LB-121** query packs over
  DuckDB · **LB-122** AI enrichment (owner gate) · **LB-123** awareness layer.
- **LB-130** Zig WASM rebuild-provenance script + CI · **LB-131** vision
  sidecar lifecycle in lavender-server · **LB-132** Nim CEP retire-or-wire
  decision.
- Release track (Flokzy-owned): ENG-996–999 desktop, ENG-1000–1004
  measurement/CI, ENG-1005–1009 guides/deploy/licence/legacy-retirement.

## In Review

- (none — ENG-974/976/983/1003 remain in review in Flokzy from before this
  board; see `task-index.md`.)
