# Why not just wrap it in an atomic?

`tach::OrderedInstant` makes a cross-thread guarantee — a timestamp read after an
`Acquire`-load on another thread's published value is never smaller than that value
— and it does so by trusting a CPU instruction barrier (`lfence; rdtsc` on x86,
`isb sy` on aarch64). The obvious question:

> Why trust a barrier? Why not guarantee monotonicity the brute-force way — wrap
> every read in a process-global `AtomicU64::fetch_max`, so every value returned is
> forced `>=` every value any thread has ever published?

We built exactly that and measured it head-to-head. The barrier wins. This note
records why, so the question doesn't have to be re-litigated.

## We built it

The atomic approach was a real type in tach during development (it was called
`SyncedInstant`). Every `now()` did the bare counter read, then
`GLOBAL_LAST_TSC.fetch_max(tsc, AcqRel)`, returning `max(tsc, previous)`. That
construction is monotone across threads by definition: the global only ever
increases, and `AcqRel` orders each fetch_max against every other.

So there were two candidates for the same guarantee: a **barrier** (`OrderedInstant`)
and a **global atomic** (`SyncedInstant`). We benchmarked both on the same hardware.

## Correctness: a tie, at zero

Both pass the cross-thread contract perfectly. Under the load-then-now-then-check
test (`measure_synchronization_order`) across the full bench matrix — 6 single-socket
cells plus pinned 2-socket Intel and AMD NUMA, ~10.9 billion reads total — **both the
barrier and the atomic recorded 0 violations.**

The atomic buys *no correctness the barrier doesn't already provide*. The whole
premise — "the barrier might let something slip; the atomic is safe" — is empirically
false. The barrier already lets nothing slip.

> Footnote, for completeness: the atomic recorded 5 per-thread backward steps on one
> cell (GitHub Windows x86_64), versus 0 for the barrier. At ~5 / 261M reads that is
> sub-ppb and is almost certainly a Q32 tick→nanosecond conversion artifact in the
> bench harness rather than the atomic's ordering failing. It is not the argument
> against the atomic — the argument is cost.

## Cost: the atomic loses, badly, under contention

A `fetch_max` is a LOCKed read-modify-write on a single process-global cache line.
With one thread that's cheap. With many threads all timestamping, that one cache line
becomes a hard serialization point — it ping-pongs between cores under the MESI
protocol, and per-call latency climbs with thread count while throughput plateaus.

The barrier has **no shared state**. Each `OrderedInstant::now()` touches only its own
core's pipeline. Its per-call cost is flat regardless of how many threads are doing it.

Measured on Apple Silicon (M-series), tight loop, atomic-based `SyncedInstant`:

| Threads | Per-call latency | Scaling efficiency* |
|--------:|-----------------:|--------------------:|
| 1 | ~4 ns | 1.00 |
| 2 | ~33 ns | 0.15 |
| 4 | ~131 ns | 0.04 |
| 8 | ~431 ns | 0.01 |
| 12 | ~892 ns | 0.01 |
| 24 | ~1745 ns | 0.00 |

\* throughput(N) / (N × throughput(1)); 1.00 = perfect scaling, 0 = fully serialized.

The atomic stops scaling at **2 threads** — past that it is slower per call than even
`std::time::Instant`, the thing tach exists to beat. By 24 threads it is ~400× its own
single-thread latency. The barrier, on the same machine and the same thread counts,
holds flat single-digit-nanosecond per-call cost.

A runtime with many threads stamping shared timestamp fields — the exact workload that
*needs* a cross-thread-correct clock — is the exact workload where the atomic collapses.

## Conclusion

The barrier is **as correct** (0 violations across ~10.9B reads, both approaches), it
is **faster** in the uncontended case, and it **has no contention cliff** because it
holds no shared state. There is no regime in which paying for the global atomic buys
anything. tach ships the barrier: `OrderedInstant`.

See `benches/ORDERED-VERIFICATION.md` for the full cross-thread correctness data.
