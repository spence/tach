# AGENTS.md

This is the canonical agent instruction file for tach. The floor to current work
is three reads: this file → [docs/STATUS.md](docs/STATUS.md) → the active
objective named there. [docs/README.md](docs/README.md) is the documentation
constitution to consult when deciding where a record belongs.

## What tach is

tach is a Rust timer crate with three `Instant`-shaped values:

- `Instant` measures local wall-rate elapsed time.
- `OrderedInstant` measures wall-rate elapsed time ordered across the documented
  synchronization edge.
- `ThreadCpuInstant` measures scheduled CPU time on the current OS thread, or a
  reported monotonic-wall fallback when that platform cannot expose thread CPU time.

The recurring classification question is which timer contract a request needs.
Choose the smallest contract that is correct; do not make a local timestamp sound
cross-thread safe, and do not make a wall fallback sound like CPU accounting.

<!-- compass-core -->
## Compass core — the north star, always in context

### Mission

- Give Rust users the fastest honest elapsed-time timer for each documented timing contract.
- Keep those timers reliable across every advertised platform and architecture.
- Make performance and correctness claims reproducible instead of rhetorical.

### Vision

Every advertised target receives the fastest eligible reliable timer for its timing
contract · every public timing claim is reproducible from frozen evidence · users
can choose and ship tach from one clear, verified release story · future timer
ergonomics extend these contracts without weakening them.

| Bar | Target |
|---|---|
| Provider coverage | Every advertised target identity compiles and its eligible routes are accounted for |
| Default selection | The selected route is eligible, reliable, and fastest or materially tied by the declared rule |
| Public claims | 0 speed or correctness claims without frozen, tracked provenance |
| Release safety | 0 package, documentation, or approval checks skipped before publication |

### Hard boundaries — out of scope

- Not a general scheduler, profiler, tracing system, or billing-policy crate.
- Not a promise that all timer values share one time domain or comparison contract.
- Not a source of silently substituted thread CPU time; a wall fallback remains explicit.
- Not a reason to weaken a correctness or performance gate after a disappointing result.

### Principle headlines

- P1 — Fastest means fastest eligible and reliable provider for the timer's exact contract.
- P2 — The three timer contracts stay distinct even though all return `Duration`.
- P3 — Capability bits nominate candidates; measured complete-path cost chooses among them.
- P4 — A platform-wide claim requires a frozen source revision and reproducible evidence.
- P5 — A release is published only after explicit owner approval.

### Judgment method

1. **Top-down** — choose the option that directly advances the Vision and P1–P5.
2. **Bottom-up** — find the actual provider, architecture, or evidence failure and reuse the
   existing principle rather than inventing a local exception.
3. **Never block on a blocker** — record an owner-only decision in `docs/ESCALATIONS.md` when
   that queue exists, then work another unblocked gate. Do not invent a green.
<!-- /compass-core -->

## Unattended-mode policy

Work autonomously within the user's stated scope. An owner-only decision is an escalation, not
permission to weaken a gate, silently publish, or reclassify a failure. A failed gate is a
finding: apply its named fallback and record the evidence.

## Source priority

1. The user's latest explicit instruction.
2. `AGENTS.md`, including the compass core.
3. [docs/STATUS.md](docs/STATUS.md).
4. [docs/README.md](docs/README.md).
5. The active plan or objective named by STATUS.
6. Load-bearing ADRs under `docs/decisions/`.
7. Existing code and tests.

Reading order is not authority order: STATUS routes work, while the active objective's milestone
table owns its live internal state.

## Operating principles

- Prefer the smallest decisive check that removes a high-leverage uncertainty, then act on its
  result rather than accumulating optional polish.
- Treat direct/provider-path parity, runtime selection, and public `now() + elapsed()` behavior as
  separate proof obligations.
- Do not call a route fast merely because a capability bit is set. Measure the whole hot path where
  selection is runtime-dependent.
- Nothing closes by assertion: a green gate needs the command result, a committed SHA, and tracked
  evidence. An objective additionally needs owner acceptance.

## Working tree discipline

- The tree may contain work from other streams. Stage only explicit paths for one verified work
  stream; never use broad staging, destructive reset/checkout, or stash commands.
- Keep source, benchmark/evidence, CI, and public-documentation changes in reviewable commits.
- Do not commit generated benchmark evidence, charts, or public claims until they are bound to the
  frozen source they describe.
- Do not push, tag, or publish a release without the user's explicit approval.

## Build and verification hygiene

- Run the normal Rust format, test, and lint gates after a source change; exercise default and
  `--no-default-features` where the changed code can affect either surface.
- Read the active objective before changing a provider, benchmark harness, evidence artifact, or
  public claim. Update its source table and Working Log in the same pass when a gate changes.
- Treat `benches/` JSON, charts, and raw run output as provisional until the validators bind them
  to the current commit. Never use old evidence to close a new-source gate.

## Living references — discovery before editing

The current objective and ADRs are the live reference map. If `reference-map.toml` is added,
consult its matching entry before editing a mapped path and honor any same-change contract.

## Subagent expectations

- Before acting, read `AGENTS.md`. Follow those repository constraints over generic persona wording.
- The parent owns synthesis, shared-tree edits, and the final verdict. Subagents return bounded,
  evidence-backed results; they do not silently broaden scope.
- Serialize edits to authored objective and Working Log sources. Resolve rendered-board conflicts by
  re-rendering from those sources, never by hand-editing a render region.

## Before finishing a work turn

- If a gate status changed, update the objective's top table and append its gate-close Working Log
  entry with evidence and SHA, then run `nsr render`.
- Before treating the current objective as safe to stop, run `nsr hold`: exit `1` means an
  actionable gate remains and work must continue; exit `0` means every open gate is explicitly
  parked behind an escalation or unlanded dependency.
- Run `nsr check`; never hand-edit a `render:` region.
- Record durable choices as ADRs and frozen failed explorations as investigations.
- Verify every changed code or documentation surface with its applicable local gate before claiming
  completion.

## Where load-bearing decisions live

- [docs/STATUS.md](docs/STATUS.md) — current objective and next objective.
- [docs/ROADMAP.md](docs/ROADMAP.md) — priority order for all remaining objectives.
- [docs/README.md](docs/README.md) — document taxonomy and evidence discipline.
- [docs/objectives/](docs/objectives/) — live milestone and gate truth.
- [docs/decisions/](docs/decisions/) — durable architectural and product rulings.
