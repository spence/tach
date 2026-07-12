# 0.3.0 plan: `ProcessInstant` + `AtomicDeadline`

Status: **planned, not active.** Target release: 0.3.0. Additive (no breaking
change). This is a retained implementation proposal under `OBJ-PROCESS-INSTANT`;
it does not authorize implementation. Revalidate its contract with an ADR before
0.3.0 work begins.

## Why

A downstream consumer (`runtime_harness_evaluation`, see its `TIMER_CONTEXT.md`)
hand-rolls two shared-time patterns that tach's value types (`Instant` /
`OrderedInstant`) structurally can't express. We analyzed both against its real call
sites and converged on exactly two additive types — deliberately **not** a general
`AtomicInstant` (see "Considered and rejected").

The core insight: `Instant` / `OrderedInstant` produce correct timestamp *values*.
What the consumer also needs is (1) a process-start instant it can declare as a
`static` without `LazyLock` boilerplate, and (2) a *shared, mutable* timestamp cell
(a re-armable deadline) — the one case that genuinely needs an atomic because there
is a writer. Neither is a new *timing* capability; they're storage/ergonomics.

## Type 1: `ProcessInstant`

A lazily-captured process-start instant. Replaces the consumer's:

```rust
static WALL_TIME_START: LazyLock<OrderedInstant> = LazyLock::new(OrderedInstant::now);
pub fn wall_time_us() -> u64 {
    u64::try_from(WALL_TIME_START.elapsed().as_micros()).unwrap_or(u64::MAX)
}
```

with:

```rust
static START: tach::ProcessInstant = tach::ProcessInstant::new();
pub fn wall_time_us() -> u64 {
    START.elapsed().as_micros() as u64
}
```

**Why it can't be simpler (no `const now()`):** `OrderedInstant::now()` reads a
hardware register (`rdtsc` / `mrs cntvct_el0`) at *runtime*; `const` runs at
*compile time*. A process-start instant in a `static` therefore *must* be lazily
captured — that's physics, not API choice. `ProcessInstant` owns the capture so the
call site is a plain `static` with no `LazyLock`/`OnceLock`.

**Cost vs `LazyLock` (honest):** equal-or-slightly-cheaper. `LazyLock::deref` does an
**acquire** load of its init-state + a branch + an **indirect** value read through a
reference (it guards an arbitrary `T`). `ProcessInstant` is `#[repr(transparent)]`
over the `u64` itself, so it does a single **relaxed** load + predicted branch + uses
the `u64` by value. Relaxed is sound here because there's no separate payload whose
visibility must be gated — an aligned `u64` load can't tear, and the value is either 0
(uncaptured) or the real tick. The win is **mainly ergonomic**; perf is a wash,
dominated by the `now()` barrier read inside `elapsed()` either way.

**Not a new cost class:** this is the same lock-free first-touch idiom
`arch::nanos_per_tick_q32()` already runs on the elapsed path
(`src/arch/mod.rs:32` — relaxed load → if 0, compute + store). `ProcessInstant` adds a
second instance of an existing tiny cost.

### API
```rust
pub struct ProcessInstant(core::sync::atomic::AtomicU64);   // #[repr(transparent)]; 0 = uncaptured

impl ProcessInstant {
  pub const fn new() -> Self;            // const → declarable as a `static`
  pub fn get(&self) -> OrderedInstant;   // the captured process-start instant (captures on first call)
  pub fn elapsed(&self) -> Duration;     // ticks_to_duration(ticks_ordered() - captured)
}
impl Default for ProcessInstant { /* = new() */ }
```
- Private `get_ticks(&self) -> u64`: relaxed load; if 0, `compare_exchange` the result
  of `arch::ticks_ordered()` (ordered read, so the epoch sample is barrier-clean); a
  losing racer re-reads the winner's value. Benign double-sample is fine (both are
  ~the same instant; document it).
- `elapsed()` saturates if the current read precedes the epoch (mirror
  `OrderedInstant::elapsed`'s `saturating_sub`).
- `#[must_use]` on `get`/`elapsed`.

## Type 2: `AtomicDeadline`

A shared, re-armable future cutoff. Replaces the consumer's
`must_yield_at_us: AtomicU64` (scheduler arms it; workers read it in a hot loop;
`now >= deadline` → request yield; `0` = disarmed by convention spread across sites).

```rust
static YIELD_AT: tach::AtomicDeadline = tach::AtomicDeadline::disarmed();

// scheduler thread:
YIELD_AT.arm(Duration::from_millis(5));

// worker hot loop (sample now() ONCE, check against it):
if YIELD_AT.is_expired(OrderedInstant::now()) { request_yield(); }
```

**This is the one genuinely mutable-shared case** — there *is* a writer (the arm), so
it needs an atomic. Reads (the frequent op) are plain atomic **loads** that scale
across cores (no exclusive-line bouncing); the write (the arm) is infrequent and
single-writer. This is **not** SyncedInstant-style: there is **no RMW on any path**.

**No `fetch_max`:** that was SyncedInstant's hammer for forcing monotonicity across
*competing writers*. A deadline has one writer arming + readers — `store`/`load`, never
"keep the max." Verified: no call site does `fetch_max` on a timestamp (they
`fetch_add` *virtual-time*, which is a different domain that stays raw `AtomicU64`).

**Backed by `OrderedInstant` for type-consistency:** the consumer's wall time is
already OrderedInstant-based, so cutoff and the `now` it's compared against are the
same type — directly comparable, no "which clock?" footgun. The barrier is free
because the caller samples `now()` anyway. (For a coarse µs–ms deadline the barrier is
a safety margin, not load-bearing — but consistency wins.)

### API
```rust
pub struct AtomicDeadline(core::sync::atomic::AtomicU64);   // #[repr(transparent)]; 0 = disarmed

impl AtomicDeadline {
  pub const fn disarmed() -> Self;                 // const → static-declarable
  pub fn arm(&self, dur: Duration);                // store(ticks_ordered() + duration_to_ticks(dur), Release)
  pub fn arm_at(&self, cutoff: OrderedInstant);    // store(cutoff ticks, Release)
  pub fn disarm(&self);                            // store(0, Release)
  pub fn is_armed(&self) -> bool;                  // load(Acquire) != 0
  pub fn is_expired(&self, now: OrderedInstant) -> bool;             // armed && now >= cutoff
  pub fn remaining(&self, now: OrderedInstant) -> Option<Duration>;  // None if disarmed or already past
  pub fn cutoff(&self) -> Option<OrderedInstant>;  // None if disarmed
}
impl Default for AtomicDeadline { /* = disarmed() */ }
```
**Decided:** encoded Acquire/Release only — `arm`/`disarm`/`arm_at` use Release,
`is_*`/`remaining`/`cutoff` use Acquire. No raw `load/store(Ordering)` escape hatch:
encoding the policy is the whole point, and the coarse Relaxed cases (e.g.
`last_used_us` idle-eviction) aren't deadlines and stay raw `AtomicU64` in the consumer.

`is_expired`/`remaining` take `now` as a **parameter** (caller samples `now()` once,
checks many deadlines) — matches `deadline.expired_at(now)` and the hot-loop pattern.

**Sentinel edge cases:** `0` means disarmed. `arm` with a duration overflowing ticks →
saturate to `u64::MAX` (never 0). `arm_at` a cutoff whose raw tick is 0 (effectively
impossible in practice) → store `1` so "disarmed" stays unambiguous.

## Considered and rejected

- **General `AtomicInstant`** — the only *other* mutable-timestamp atomics in the
  consumer are `last_used_us` (Relaxed, coarse idle-eviction) and `signal_ns` /
  `script_end_ns` (bench-only). Neither warrants a named type; they stay raw
  `AtomicU64`. Revisit only if a real "shared past-event high-water-mark" need with
  `fetch_max` appears (that's the one op a general AtomicInstant would add over these
  two types). minstant/fastant ship an `Atomic<Instant>` with `fetch_max`, but that's
  a build-it-yourself cell, not something our call sites use.
- **`ProcessClock` / `Timestamp` newtype** — the broader `TIMER_CONTEXT.md` wishlist.
  `ProcessInstant` covers the concrete epoch need without the larger multi-method
  surface; defer the rest until there's demand.
- **`const now()` / `ctor`-style auto-init** — impossible (runtime register read) /
  rejected (dependency + run-before-main hazards). Lazy first-touch is the right model.

## Implementation notes (confirmed against the source)

- Both types live in **`src/instant.rs`** (alongside `Instant`/`OrderedInstant`) → they
  reach the private `.0` tick directly and call the **file-private** helpers
  `ticks_to_duration` / `duration_to_ticks` (`src/instant.rs:389` / `:404`) and
  `arch::ticks_ordered()`. **No new public accessors or visibility changes.**
- `core::sync::atomic::{AtomicU64, Ordering}` already used unconditionally in
  `src/arch/mod.rs` → no new dependency, no `target_has_atomic` concern,
  `#![no_std]`-clean. (Keep `AtomicU64`, not `AtomicUsize` — 32-bit x86 would truncate.)
- Mirror the first-touch idiom from `nanos_per_tick_q32`.

### Files
- `src/instant.rs` — add both types.
- `src/lib.rs` — `pub use instant::{Instant, OrderedInstant, ProcessInstant, AtomicDeadline};`
  + tests.
- `README.md` — replace any hand-rolled `LazyLock<OrderedInstant>` example with
  `ProcessInstant`; add the `AtomicDeadline` yield example. There is one canonical
  public README, not a mirrored crate-specific copy.
- Bump `Cargo.toml` to 0.3.0 when shipping.

### Tests (`src/lib.rs mod tests`, undecorated per styleguide)
- `process_instant_and_deadline_send_sync`
- `process_instant_elapsed_advances` — two reads monotone; after 10ms sleep elapsed ≥ 9ms
- `process_instant_concurrent_first_touch` — N threads race first `.elapsed()`; all land
  in a tight band (single consistent epoch, no torn capture)
- `deadline_arm_expires_after_sleep` — armed 5ms: not expired now, expired after sleep;
  `remaining` shrinks
- `deadline_disarm` — `disarm` → `is_armed()==false`, `is_expired(now)==false`, `cutoff()==None`
- `deadline_cross_thread_visibility` — thread A `arm`s (Release); thread B, after a sync
  edge, `is_expired` (Acquire) sees the armed deadline

### Verification
```
cargo build --lib                                   # no_std: const ctors + AtomicU64 without std
cargo test --lib --tests --features "bench-internal recalibrate-background"
cargo test --doc
cargo clippy --lib --all-targets -- -D warnings
```

## Commit
Single `feat(api): add ProcessInstant and AtomicDeadline` + the 0.3.0 version bump.
Additive; no breaking change.
