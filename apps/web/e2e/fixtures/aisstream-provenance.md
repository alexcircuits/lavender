# `aisstream.jsonl` — synthetic AIS replay

**Synthetic, not real ship data.** Every vessel, MMSI, IMO number, name, destination and
position in `aisstream.jsonl` was authored on 2026-10-15 for Lavender's tests. No AISStream
key was available, so nothing here was recorded from the live service; MMSIs use invented
numbers (`230990…`, `276990…`, `512990…`) and IMO numbers are made-up values with valid check
digits. Any resemblance to a real vessel is accidental.

Format: one AISStream WebSocket message per line, exactly as the service sends them (UTF-8
JSON), following the aisstream.io message models (`AisStreamMessage`, `PositionReport`,
`StandardClassBPositionReport`, `ShipStaticData`, `SubscriptionConfirmation`), after one
first line that is Lavender's own and not an AISStream message:
`{"MessageType":"LavenderFixtureProvenance","Synthetic":true,…}`. The AISStream parser ignores
it like any message type Lavender does not subscribe to; fixture mode reads it to give vessel
snapshots the notice "Synthetic AIS replay: invented test ships at their recorded report
times." A replay without that line (for example a real capture placed in another fixtures
directory) gets "Vessel positions keep their recorded report times." instead.

- https://aisstream.io/documentation (checked 2026-10-15)
- https://github.com/aisstream/ais-message-models/blob/master/type-definition.yaml (checked 2026-10-15)

Replay (`lavender_core::providers::aisstream::replay`, used by fixture mode and
`crates/lavender-core/tests/vessel_replay.rs`) runs every line through the real parser and
store, using each report's own `time_utc` as the clock, so the recording keeps its times
(2026-10-15 11:20–12:00 UTC) and is never shown as live.

What the recording covers:

| Case | Ships |
|---|---|
| Moving ship with a seven-point track, static data after positions | `230990001` TEST TRADER (cargo) off Helsinki |
| Static data before the first position, four-point track | `230990002` TEST FERRY (passenger), Helsinki–Tallinn lane |
| Ship at anchor (navigational status 1; the other Class A reports say 0, under way using engine): repeated identical fixes keep one track point | `276990003` TEST TANKER off Tallinn |
| Class B report named only by stream metadata | `230990004` TEST SAILOR |
| Speed 102.3, course 360 and heading 511 ("not available") | `230990005` TEST SENTINEL |
| Position 91/181 ("not available"): never placed | `230990006` TEST NO FIX |
| Heard only at 11:20, older than the 30-minute retention at 12:00: removed | `230990007` TEST OLD TIMER |
| Either side of the antimeridian | `512990008` TEST EAST, `512990009` TEST WEST (near Fiji) |
| Static data only: known, not shown | `230990010` TEST GHOST |
| Ignored message types | the `LavenderFixtureProvenance` line, `SubscriptionConfirmation`, a `BaseStationReport` |
| Malformed frames, each dropped alone | not JSON; MMSI with ten digits; `Valid: false`; latitude 95 |

After replay the vessels feed shows seven ships; the store knows eight.
