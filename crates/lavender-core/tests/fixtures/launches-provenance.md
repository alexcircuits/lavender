# Launch fixtures (ENG-957)

## Recorded

Downloaded on 2026-10-15 for the launch-to-satellite matching tests. Values are unchanged;
only unused parts were removed, as described.

- `launches-ll2-recorded.json`: Launch Library 2 response downloaded at
  2026-10-15T20:31:16Z from
  `https://ll.thespacedevs.com/2.3.0/launches/?net__gte=2026-10-09T00:00:00Z&net__lte=2026-10-20T00:00:00Z&ordering=net&limit=10&mode=detailed`.
  All eight launches are kept: five already launched (2026-230 to 2026-234) and three still
  upcoming at download time. Trimmed: per launch only `id`, `url`, `name`, `slug`,
  `launch_designator`, `status`, `last_updated`, `net`, `net_precision`, `window_end`,
  `window_start`, `probability`, `failreason`, `launch_service_provider` (`response_mode`,
  `id`, `url`, `name`, `abbrev`, `type`), `rocket` (`id`, `configuration` reduced to
  `response_mode`, `id`, `url`, `name`, `full_name`, `variant`, and `payloads`), `mission`
  (`id`, `name`, `type`, `description`, `orbit` reduced to `id`, `name`, `abbrev`) and
  `pad` (`id`, `url`, `active`, `name`, `latitude`, `longitude`, `location` reduced to
  `response_mode`, `id`, `url`, `name`, `timezone_name`, `latitude`, `longitude`) remain.
  Images, agencies' details, launcher stages, timelines, updates and video links were
  removed. `count`, `next` and `previous` are as served.
- `celestrak-last-30-days-recorded.json`: CelesTrak OMM JSON downloaded at
  2026-10-15T20:31:18Z from
  `https://celestrak.org/NORAD/elements/gp.php?GROUP=last-30-days&FORMAT=json`. The whole
  response, byte for byte as served (119 688 bytes, 282 objects from 25 launches, rocket
  bodies and debris included). Only launches 2026-230 to 2026-234 are in the LL2 fixture,
  so the matching tests also prove that objects of the other 20 launches (among them the
  neighbouring 2026-229) are not matched. This group is in the default CelesTrak groups
  because the operational groups do not list newly launched objects.

The same day a list-mode query of the full production window
(`net__gte=2026-09-15T20:30:53Z`, `net__lte=2026-11-14T20:30:53Z`, `ordering=net`,
`limit=100`) reported `count` 36: 24 launches before and 12 after the request time, so one
page of 100 holds the whole window. That response is not kept as a fixture.

## Synthetic

Written for these tests in the Launch Library 2 v2.3.0 response shape; the names, dates,
identifiers and statuses are illustrative, not real records.

- `launches-ll2.json`: three launches with pad coordinates and payload names, plus one
  intentionally malformed row to test drop counting. Also copied to
  `apps/web/e2e/fixtures/launches.json` for fixture mode.
- `launches-empty.json`: a valid response with zero results.
- `launches-malformed.json`: a response without the required `results` array.

## Terms

- Launch Library 2 is provided by The Space Devs. Their terms of use
  (https://github.com/TheSpaceDevs/Tutorials/blob/main/faqs/faq_TSD.md#terms-of-use) allow
  using and sharing the data in any form, ask users not to forward it without adding value,
  and encourage but do not require attribution. They name no Creative Commons licence.
  Credit: "Launch Library 2 — The Space Devs".
- CelesTrak element sets are US-government-origin data with no licence; CelesTrak asks
  for citation (as recorded in `legacy/DATA_SOURCES.md`). Credit: "CelesTrak
  (celestrak.org), Dr. T.S. Kelso".
