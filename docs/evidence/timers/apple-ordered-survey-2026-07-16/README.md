# `EVID-APPLE-ORDERED` — Apple `OrderedInstant` happens-before survey: which counter reads carry the cross-thread ordering edge, on M1 Max and M4 Pro (2026-07-16)

**Status: EVIDENCE FROZEN — resolves `ESC-APPLE-ORDERED-SELECTION` concerns 2 and 3. The
provider philosophy choice (fastest self-sync vs conservative barrier vs match-quanta) remains an
open owner ruling; this evidence informs it but does not make it.**

## Question

`ESC-APPLE-ORDERED-SELECTION` raised three entangled doubts about the Apple `OrderedInstant`
provider, the sharpest being concern 3: the runtime-selected provider `mach_absolute_time()` is a
bare FFI call with no `isb`/`dmb`/`dsb`, yet `ordered_honors_happens_before` certifies 0
violations — an unexplained inconsistency. Before freezing a fixed pick (M2), we must know which
candidate reads actually satisfy the cross-thread happens-before contract, measured, not inferred.

## Method

`#[ignore]`'d test `arch::apple_aarch64::tests::ordered_candidate_happens_before_survey`
(committed `3f9050e`, duration-parameterized `1e7ec88`). Every hardware thread runs a stress loop:
`Acquire`-load the max published tick, read the candidate, count a violation if the read is less
than the already-observed maximum, then `Release` `fetch_max` the read back. `bare_cntvct` is the
negative control (unbarriered — must fail); `isb+cntvct` is the positive control (explicit barrier
— must hold). A candidate that reads below a tick another thread already published is a
happens-before violation.

## Provenance

- Repo SHA: `1e7ec88` (survey probe + `TACH_SURVEY_SECS` parameterization; the measured reader
  bodies are unchanged since the frozen `EVID-APPLE-BARE-CNTVCT` tree).
- Hardware: `catalyst` — MacBook Pro M1 Max; `catalyst-mini` — Mac mini M4 Pro; both
  `aarch64-apple-darwin`, both `user_timebase_mode=3` (Apple self-synchronizing register
  designated), `cont_hwclock=1`. Both ran `acntvct` (mode-3-only register) without trapping,
  confirming mode 3 on each.
- Command: `TACH_SURVEY_SECS=30 cargo test --lib ordered_candidate_happens_before_survey --
  --ignored --nocapture` (10 threads M1 Max, 12 threads M4 Pro, 30 s/candidate).

## Results — violations / reads

| Candidate | read form | M1 Max | M4 Pro | verdict |
|---|---|---|---|---|
| `bare_cntvct` | `mrs cntvct_el0`, no barrier | **50,353,599 / 467,657,313 (10.8%)** | **59,374,816 / 475,201,172 (12.5%)** | ❌ NOT ordered (control fires) |
| `mach_absolute` | `mach_absolute_time()` FFI call | **0 / 439,206,333** | **0 / 446,938,835** | ✅ ordered |
| `acntvct` | `ACNTVCT_EL0` self-sync + offset | **0 / 440,358,205** | **0 / 455,456,627** | ✅ ordered |
| `isb+cntvct` | `isb sy; cntvct` + offset | **0 / 424,415,506** | **0 / 427,949,023** | ✅ ordered (positive control) |

Combined across both machines: bare `112.7M / 942.9M`; each ordered candidate `0 / ~0.87e9`.

## Findings

- **Concern 3 resolved.** The negative control fires hard (>110M violations across ~0.94e9 reads),
  so the harness detects real violations. Against that, `mach_absolute`, `acntvct`, and `isb+cntvct`
  each hold exactly 0 across ~0.87e9 reads per machine — an empirical failure rate below ~1.2e-9.
  The `mach_absolute_time()` read is ordered because the ordering the contract needs is that the
  timer read not be hoisted above a prior `Acquire` load; the opaque FFI call boundary supplies
  that, and both self-synchronizing forms supply it architecturally. This is the same
  measurement-based eligibility bar `Instant`'s bare `CNTVCT` cleared in `EVID-APPLE-BARE-CNTVCT`
  (0 / 2.8e9).
- **A faster-than-`isb` ordered provider exists.** `acntvct` (Apple self-synchronizing register)
  is ordered *and*, per frozen `EVID-APPLE-BARE-CNTVCT` line "Speed", reads at 5.06/3.55 ns vs
  `mach` 5.38/3.78 ns vs `isb+cntvct` 10.22/8.66 ns (M1/M4). ARM defines the self-synchronizing
  counter as equivalent to `isb`-preceded `CNTVCT`, so `acntvct` carries the *same* ordering
  guarantee as the explicit barrier without the pipeline flush.
- **Concern 2 (sleep-domain coin-flip) dissolves under any fixed pick.** All three offset-corrected
  ordered candidates land in the Mach absolute (exclude-sleep) domain. A deterministic fixed pick
  ends the cross-start `mach_absolute`/`mach_continuous` non-determinism the runtime tournament
  exhibited.

## Open (owner)

- **Provider philosophy** (`ESC-APPLE-ORDERED-SELECTION`): (A) fastest measurement-eligible —
  `acntvct` self-sync gated to `isb+cntvct` where the CPU mode exposes no self-sync register (the
  x86-LFENCE-gate pattern); (B) conservative — always `isb+cntvct`; (C) match quanta —
  `mach_absolute_time()`. All three are ordered-eligible by this evidence; they differ in speed and
  in whether the guarantee is architectural or call-boundary. Recommendation: A.
- Sleep-domain suspend semantics (§5.1 item d) remain a separate owner-coordinated documentation
  run, unchanged by this survey.

## Reproduce

```
TACH_SURVEY_SECS=30 cargo test --lib ordered_candidate_happens_before_survey -- --ignored --nocapture
```

## Raw artifacts

- [`survey-catalyst-m1max.txt`](survey-catalyst-m1max.txt) — M1 Max run (10 threads).
- [`survey-catalyst-mini-m4pro.txt`](survey-catalyst-mini-m4pro.txt) — M4 Pro run (12 threads).
