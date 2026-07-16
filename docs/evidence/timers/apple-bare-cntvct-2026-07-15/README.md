# `EVID-APPLE-BARE-CNTVCT` — Apple bare CNTVCT_EL0 re-adjudication: correctness, domain, and speed on M1 Max and M4 Pro (2026-07-15)

**Status: ADOPTED — bare `CNTVCT_EL0` is Apple `Instant`'s selected provider; `OrderedInstant` unchanged. Suspend/wake documentation run remains open.**

## Provenance

- Repo SHA(s): probes ran against the standalone `probe.rs` (no crate dependency); the
  post-adoption criterion run measured the working tree whose parent is `037f49c` (the adoption
  commit's diff is exactly the bare-counter candidate + provider-aware instant scale).
- Substrate / hardware: `catalyst` — MacBook Pro M1 Max, macOS, `aarch64-apple-darwin`;
  `catalyst-mini` — Mac mini M4 Pro, macOS, `aarch64-apple-darwin`. Both report
  `user_timebase_mode=3` (Apple self-synchronizing register designated), `cont_hwclock=1`.
- Command surface: `rustc -O probe.rs` + direct execution on both machines;
  `cargo bench --bench instant --features bench-quanta -- --warm-up-time 1 --measurement-time 2`
  (filtered) on catalyst.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| Same-thread monotonicity of the bare read | 🟢 0 violations in ~1.46e9 reads (M1 Max) + ~1.31e9 reads (M4 Pro), single-thread pairs and 8-thread migrating loops | [`probe-catalyst-m1max.txt`](probe-catalyst-m1max.txt) · [`probe-catalyst-mini-m4pro.txt`](probe-catalyst-mini-m4pro.txt) |
| Wall rate | 🟢 worst ratio vs `std` 0.99997 (M1); M4 exact at 1 GHz once the counter's own frequency is used | same artifacts |
| Tick domain | 🟢 M1: bare == mach_absolute == mach_continuous (delta 0, ratio 1.000000). M4: bare is its own domain — 1 GHz vs 24 MHz Mach ticks — so the instant scale must follow `CNTFRQ_EL0` (truthful on both machines) | same artifacts |
| Speed | 🟢 bare read 0.33 ns (M1) / 0.44 ns (M4) vs designated self-sync register 5.06/3.55 ns, prior selected route 5.38/3.78 ns, `isb+cntvct` 10.22/8.66 ns | same artifacts |
| Post-adoption public path | 🟢 `Instant::now()` 0.93 ns (was 7.79), roundtrip 2.27 ns (was 15.47); quanta same-run 3.30/7.22; selector chose `apple_bare_cntvct`; inline parity within `max(1 ns, 5%)` | [`criterion-catalyst-post-adoption.txt`](criterion-catalyst-post-adoption.txt) |

## Findings resolved here

- ADR-0003's `Instant`-scope exclusion of the bare counter dissolves under ADR-0005: no
  class-1/class-2 basis survived — the bare read satisfies the published same-thread monotonic
  wall-rate contract on both tested generations, at ~8–15× lower read cost than every XNU
  protocol route.
- The May-era 0.35 ns Apple benchmark was real unserialized-read throughput, not a harness
  artifact; the July campaign's 7.5 ns floor came from restricting candidates to the
  commpage-designated self-synchronizing registers.
- The M1↔M4 frequency divergence (24 MHz vs 1 GHz) is a real same-target cross-machine
  divergence: any fixed-pick implementation must scale by `CNTFRQ_EL0` per machine, never by a
  constant or the Mach timebase.
- `OrderedInstant` keeps the self-synchronizing route: `isb sy; cntvct` measured ~2× slower than
  the selected ordered provider on both machines, and an unbarriered read is never
  synchronization-ordered.

## Open

- Suspend/wake documentation run (plan §5.1 item d): formal ×5 sleep cycle on catalyst pending an
  owner-coordinated window. De facto evidence already on file: on M1 the live machine shows
  `mach_absolute == mach_continuous == bare` with delta 0 after months of daily sleep cycles, so
  the bare counter advances through sleep exactly like both Mach timelines. Not adoption-blocking
  under ADR-0005 (suspend semantics are documentation, not eligibility).
- Full-crate test battery on `catalyst-mini` (raw-read probes ran there; the crate suite has only
  run on catalyst). Executor completes during M1.
- Mode-2 hardware (non-Apple `CNTVCTSS` guidance) has no bare-read evidence; the candidate is not
  offered there.

## Reproduce

```
rustc -O -o /tmp/probe docs/evidence/timers/apple-bare-cntvct-2026-07-15/probe.rs && /tmp/probe
cargo bench --bench instant --features bench-quanta -- \
  'bare_cntvct|hw_acntvct_base|^Instant::now\(\)/tach$|/quanta$' \
  --warm-up-time 1 --measurement-time 2
```

## Raw artifacts

- [`probe.rs`](probe.rs) — standalone monotonicity/rate/domain/cost probe (no dependencies).
- [`probe-catalyst-m1max.txt`](probe-catalyst-m1max.txt) — M1 Max run output.
- [`probe-catalyst-mini-m4pro.txt`](probe-catalyst-mini-m4pro.txt) — M4 Pro run output (annotated:
  the probe hardcodes 24 MHz, the 41.666 ratio decodes to an exactly-1 GHz counter).
- [`criterion-catalyst-post-adoption.txt`](criterion-catalyst-post-adoption.txt) — filtered
  criterion medians on the adopted working tree.
