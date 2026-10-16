# `launches.json` provenance

**Synthetic.** A byte-for-byte copy of
`crates/lavender-core/tests/fixtures/launches-ll2.json` (see `launches-provenance.md` there):
three illustrative Launch Library 2 v2.3.0-shaped launches (Cape Canaveral and Vandenberg
pads, June and August 2026 dates) plus one malformed row the parser must skip. The names,
identifiers, dates and statuses are not real records, and the launches carry no COSPAR
designator, so fixture mode matches no satellites to them. Fixture mode
(`crates/lavender-core/src/providers/fixture.rs`) serves them with the notice "Recorded
fixture data, not live."
