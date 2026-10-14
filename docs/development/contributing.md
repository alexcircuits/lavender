# Contributing

## Choose work from Flokzy

Open the [Lavender project](https://app.flokzy.com/app/projects/3cf1d056-2bd8-4cb7-9381-c83d58fb1bb2).
The [roadmap](../roadmap.md) suggests a sensible order; the [Task Index](task-index.md)
is a dated snapshot, not a substitute for current status.

1. Read the task description, comments, owner and dependency links. Check all
   `blocked_by` tasks are completed and any provider key/terms are available.
2. Choose an unclaimed **Todo** deliverable, not a workstream parent. Parents
   summarize related tasks and can remain Todo while children are completed.
3. Respect **In Progress** and **In Review** work. Coordinate with its owner before
   changing overlapping code. Read discussion before narrowing or splitting scope.
4. Put findings and verification on the existing task. Start substantive work in
   **In Progress**, move reviewable work to **In Review**, and close only after its
   acceptance criteria are verified and the team's review/merge rule is satisfied.

Do not create a second project or ticket for an existing migration. Dates and
ownership come from the task or the owner; missing fields are not invitations to
invent a schedule. A code inspection finding should be recorded as evidence,
without silently marking another developer's work complete.

## Before editing

Read [Start Here](start-here.md), the [Codebase Map](codebase-map.md), then the guide
for your change: [Add a Provider](add-provider.md), [Architecture](architecture.md),
[Web and Desktop](web-and-desktop.md), or the linked detailed specification.

```bash
git rev-parse --show-toplevel
git status --short
git switch -c improvement/eng-1012-readable-backend
```

The branch command is an **example naming pattern**; substitute your task ID and
change. This checkout's Lavender repository is nested in the monorepo. Check the
Git root, preserve other work and restrict changes to Lavender. The parent
monorepo requires product isolation and independent lockfiles. No Lavender-specific
mandatory branch naming or merge policy was found; agree those with the reviewer
rather than treating an example as an established company rule.

## Code conventions

Use Rust `snake_case` functions/modules, `PascalCase` types and uppercase constants.
Use names that identify the domain record or operation. `fetch_` implies external
I/O, `load_` reads stored data, `parse_` interprets input and `normalize_` creates a
validated record. Existing conventional module-qualified names such as
`adsb_lol::parse` remain clear; cosmetic breaking renames are unnecessary.

Keep provider/core behavior out of HTTP and Tauri handlers. Prefer concrete types
and direct calls. Retain traits where there are real implementations (`FeedProvider`,
secret/settings stores, frontend platform) and concurrency/security boundaries.
Explain quotas, units, cache age and cancellation constraints in comments; do not
comment every obvious statement.

Rust's checked-in lint policy forbids unsafe code and denies unwrap/expect/panic
in production, with documented test/constant exceptions. Frontend TypeScript is
strict and checked by Biome. Never edit generated TS by hand; change Rust contracts
and regenerate them. Keep exact dependency pins and review lockfile/license changes.
No source imports into another NoirSystems product are allowed.

## Review and definition of done

Keep commits focused. Explain the changed behavior or responsibility in a direct
commit subject; the existing history uses `feat:` and `chore:` but no additional
mandatory commit format is established. A PR should state the concrete problem,
result, task link, tests run and any remaining limitation. Review against the
current target branch, including generated files and notices when relevant.

Before requesting review:

- The task's observable acceptance criteria hold; planned behavior is not claimed
  as implemented. Preserve useful functionality, provenance, security and Tauri reuse.
- Run targeted tests while coding, then `./scripts/check.sh`. Run fixture E2E for
  changed feed/transport/globe integration. Include desktop checks if affected.
- Update contributor pages, migration/parity rows and source notices relevant to
  the change. Verify command examples and relative file links.
- Inspect the diff for unrelated formatting, deleted tests, secrets and accidental
  changes in legacy or other products.
- Record evidence, blockers and follow-ups in Flokzy. Leave unverified work open.

A passing local check makes a change reviewable; it does not by itself prove
production deployment, commercial clearance, full upstream parity or cross-platform
readiness. Those are separate tracked deliverables.
