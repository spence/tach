# 0006 — Apple OrderedInstant selects the self-synchronizing counter, mode-gated

- Status: Accepted
- Date: 2026-07-16
- Source: owner ruling
- Related: ADR-0005 (selection policy — this applies its capability-gate rule to the Apple ordered
  pick); ADR-0003 (ordering half stands: an unbarriered read is never ordered); `OBJ-SIMPLIFY-TIMERS.M2`;
  [`EVID-APPLE-ORDERED`](../evidence/timers/apple-ordered-survey-2026-07-16/README.md);
  resolves `ESC-APPLE-ORDERED-SELECTION`.

## Decision

Apple `OrderedInstant`'s fixed pick is the **self-synchronizing architectural counter, selected by
the XNU commpage user-timebase mode** — a correctness capability gate, not a runtime tournament:

| commpage mode | ordered read | basis of the ordering edge |
|---|---|---|
| 3 `NOSPEC_APPLE` | `ACNTVCT_EL0` + timebase offset | Apple self-synchronizing register |
| 2 `NOSPEC` | `CNTVCTSS_EL0` + timebase offset | ARMv8.6 self-synchronizing register |
| 1 `SPEC` | `isb sy; CNTVCT_EL0` + timebase offset | explicit instruction-synchronization barrier |
| 0 `NONE` | `mach_absolute_time()` | opaque libSystem call boundary |

All four read the Mach absolute (exclude-sleep) domain, so the pick is deterministic in sleep
domain. `Instant` is unaffected: bare `CNTVCT_EL0` (modes 1/3), `CNTVCTSS_EL0` (mode 2),
`mach_absolute_time()` (mode 0), scaled by `CNTFRQ_EL0` when bare, else the Mach timebase.

The self-synchronizing register carries the same happens-before guarantee as `isb`-preceded
`CNTVCT` (ARM defines the self-synchronized counter read as equivalent to an `ISB` followed by a
`CNTVCT` read) without the pipeline flush, so it is both faster and architecturally ordered.

## Why this exists (pressure / failure mode)

The retained runtime tournament coin-flipped `mach_absolute_time`/`mach_continuous_time` for the
ordered pick across process starts (`ESC-APPLE-ORDERED-SELECTION`): non-deterministic in sleep
domain, never selecting the self-synchronizing route the provider matrix claimed, and resting the
happens-before edge on an unexplained property of a bare call. Freezing a fixed pick (M2) forced
the question of *which* read actually carries the edge. A two-machine cross-thread happens-before
survey (`EVID-APPLE-ORDERED`, M1 Max + M4 Pro, SHA `1e7ec88`) settled it by measurement: the
unbarriered `bare_cntvct` control fired 112.7M violations / 942.9M reads, while `mach_absolute`,
`acntvct` (self-sync), and `isb+cntvct` each held 0 / ~0.87e9 reads per machine. The self-sync
route is fastest of the three ordered-eligible reads (5.06/3.55 ns vs mach 5.38/3.78 vs isb
10.22/8.66, M1/M4 — `EVID-APPLE-BARE-CNTVCT`) and is the only one whose guarantee is architectural
rather than empirical.

## Required invariants (what future work must preserve)

- The ordered read is never an EL0 register the current commpage mode disallows — reading an
  unpermitted counter traps (SIGILL). Mode 0 uses `mach_absolute_time()`; the gate is a correctness
  boundary, not a latency choice (per ADR-0005 capability-gate policy).
- An unbarriered bare counter read is never the ordered pick (ADR-0003 ordering half stands).
- The ordered read stays in the Mach absolute (exclude-sleep) domain; `Instant`'s read and its
  scale derive from one shared mode predicate (a bare read with a Mach scale, or the converse, is a
  silent ~40× `elapsed()` error on 1 GHz-counter parts).

## Operational consequences

- `src/arch/apple_aarch64.rs` converts to the fixed pick above and its 10-provider tournament,
  selection evidence, and continuous-hwclock readers are deleted (one Apple-family commit).
- `M2.G1` unblocks; the mode gate is retained with a prominent WHY comment (the x86 LFENCE-gate
  pattern) so it is not later removed as tournament cruft.
- The Python `validate_apple_wall_selector` still expects the tournament shape and is reconciled in
  `M3` alongside the other converted families — not an M2 regression.

## Rejected alternatives

- **Always `isb sy; cntvct` (Option B):** same guarantee, ~2× slower on all shipping Apple Silicon
  because `isb` flushes the pipeline; its relative cost grows under contention where the self-sync
  read stays flat. Simpler (no mode gate) but pays speed on every real device.
- **`mach_absolute_time` (Option C, quanta's clock):** ordered-eligible by measurement, but the
  guarantee rests on the opaque libSystem call boundary rather than an architectural counter
  property, and it is an out-of-line call whose internals Apple controls. Slightly slower than the
  self-sync read with a weaker rationale.
- **Keeping the runtime tournament:** no frozen same-target flip exists among ordered candidates
  (ADR-0005); the tournament produced the non-determinism this ruling removes.

## Verification

`OBJ-SIMPLIFY-TIMERS.M2.G1` (code matches the frozen pick on both feature surfaces; inline parity
within `max(1 ns, 5%)`; tournament symbols grep-clean). The correctness evidence is
[`EVID-APPLE-ORDERED`](../evidence/timers/apple-ordered-survey-2026-07-16/README.md); its
`ordered_candidate_happens_before_survey` reproducer is retained in the arch module.
