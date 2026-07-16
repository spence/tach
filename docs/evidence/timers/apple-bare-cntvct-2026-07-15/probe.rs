// Standalone probe: is bare CNTVCT_EL0 (and mode-designated ACNTVCT) a valid
// same-thread-monotonic, wall-rate Instant source on Apple Silicon — and what
// does each read cost? No dependencies; build: rustc -O -o probe this-file.rs
#![allow(clippy::all)]
use std::arch::asm;
use std::time::{Duration, Instant as StdInstant};

const COMM_PAGE_BASE: usize = 0x0000_000f_ffff_c000;
const USER_TIMEBASE: usize = COMM_PAGE_BASE + 0x090;
const CONT_HWCLOCK: usize = COMM_PAGE_BASE + 0x091;
const CONT_HW_TIMEBASE: usize = COMM_PAGE_BASE + 0x0a8;

extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_continuous_time() -> u64;
}

#[inline(always)]
fn bare_cntvct() -> u64 {
    let c: u64;
    unsafe { asm!("mrs {}, cntvct_el0", out(reg) c, options(nostack, preserves_flags)) };
    c
}

#[inline(always)]
fn bare_acntvct() -> u64 {
    let c: u64;
    unsafe { asm!("mrs {}, S3_4_C15_C10_6", out(reg) c, options(nostack, preserves_flags)) };
    c
}

#[inline(always)]
fn bare_cntvctss() -> u64 {
    let c: u64;
    unsafe { asm!("mrs {}, S3_3_C14_C0_6", out(reg) c, options(nostack, preserves_flags)) };
    c
}

#[inline(always)]
fn isb_cntvct() -> u64 {
    let c: u64;
    unsafe { asm!("isb sy", "mrs {}, cntvct_el0", out(reg) c, options(nostack, preserves_flags)) };
    c
}

#[inline(always)]
fn continuous_route() -> u64 {
    // The current tach winner shape: designated register + continuous base load.
    let r: u64;
    unsafe {
        asm!(
            "mrs {c}, S3_4_C15_C10_6",
            "ldr {b}, [{a}]",
            "add {r}, {c}, {b}",
            a = in(reg) CONT_HW_TIMEBASE,
            c = out(reg) _,
            b = out(reg) _,
            r = lateout(reg) r,
            options(nostack),
        )
    };
    r
}

fn mono_pairs(name: &str, f: fn() -> u64, pairs: u64) -> u64 {
    let mut violations = 0u64;
    let mut worst = 0u64;
    let start = StdInstant::now();
    for _ in 0..pairs {
        let a = f();
        let b = f();
        if b < a {
            violations += 1;
            worst = worst.max(a - b);
        }
    }
    let dt = start.elapsed();
    println!(
        "mono {name}: {pairs} pairs, {violations} violations (worst backstep {worst} ticks), {:.2} ns/read",
        dt.as_nanos() as f64 / (pairs as f64 * 2.0)
    );
    violations
}

fn mono_threads(name: &str, f: fn() -> u64, threads: usize, pairs: u64) -> u64 {
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            std::thread::spawn(move || {
                let mut violations = 0u64;
                for i in 0..pairs {
                    let a = f();
                    let b = f();
                    if b < a {
                        violations += 1;
                    }
                    if i % 4_000_000 == 0 {
                        std::thread::yield_now(); // invite migration across P/E cores
                    }
                }
                violations
            })
        })
        .collect();
    let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    println!("mono-threads {name}: {threads}x{pairs} pairs, {total} violations");
    total
}

fn throughput(name: &str, f: fn() -> u64, reads: u64) {
    let mut acc = 0u64;
    for _ in 0..1_000_000 {
        acc ^= f();
    }
    let start = StdInstant::now();
    for _ in 0..reads {
        acc ^= f();
    }
    let dt = start.elapsed();
    std::hint::black_box(acc);
    println!("cost {name}: {:.2} ns/read", dt.as_nanos() as f64 / reads as f64);
}

fn rate(name: &str, f: fn() -> u64) {
    // cntvct family ticks at 24 MHz on Apple Silicon: 125/3 ns per tick.
    let mut worst: f64 = 1.0;
    for _ in 0..20 {
        let s0 = StdInstant::now();
        let t0 = f();
        std::thread::sleep(Duration::from_millis(100));
        let t1 = f();
        let s1 = s0.elapsed();
        let ratio = ((t1 - t0) as f64 * 125.0 / 3.0) / s1.as_nanos() as f64;
        if (ratio - 1.0).abs() > (worst - 1.0).abs() {
            worst = ratio;
        }
    }
    println!("rate {name}: worst ratio vs std over 20x100ms = {worst:.5}");
}

fn main() {
    let mode = unsafe { std::ptr::read_volatile(USER_TIMEBASE as *const u8) };
    let hwclock = unsafe { std::ptr::read_volatile(CONT_HWCLOCK as *const u8) };
    let mach0 = unsafe { mach_absolute_time() };
    let cont0 = unsafe { mach_continuous_time() };
    let bare0 = bare_cntvct();
    println!("host: user_timebase_mode={mode} cont_hwclock={hwclock}");
    println!(
        "domain: mach_absolute={mach0} mach_continuous={cont0} bare_cntvct={bare0} (bare-cont delta {})",
        cont0 as i64 - bare0 as i64
    );

    // Tick-domain check: bare counter must advance 1:1 with mach ticks.
    let (m0, c0) = (unsafe { mach_absolute_time() }, bare_cntvct());
    std::thread::sleep(Duration::from_millis(500));
    let (m1, c1) = (unsafe { mach_absolute_time() }, bare_cntvct());
    println!("domain: d(mach)/d(bare) over 500ms = {:.6}", (m1 - m0) as f64 / (c1 - c0) as f64);

    let mut total_violations = 0u64;
    total_violations += mono_pairs("bare_cntvct", bare_cntvct, 500_000_000);
    total_violations += mono_threads("bare_cntvct", bare_cntvct, 8, 60_000_000);
    if mode == 3 {
        total_violations += mono_pairs("bare_acntvct", bare_acntvct, 250_000_000);
    }
    if mode == 2 {
        total_violations += mono_pairs("bare_cntvctss", bare_cntvctss, 250_000_000);
    }

    rate("bare_cntvct", bare_cntvct);

    throughput("bare_cntvct", bare_cntvct, 200_000_000);
    if mode == 3 {
        throughput("bare_acntvct", bare_acntvct, 200_000_000);
    }
    if mode == 2 {
        throughput("bare_cntvctss", bare_cntvctss, 200_000_000);
    }
    throughput("isb_cntvct", isb_cntvct, 50_000_000);
    if hwclock != 0 && mode == 3 {
        throughput("continuous_route(acntvct+base)", continuous_route, 200_000_000);
    }
    throughput("mach_absolute_time", || unsafe { mach_absolute_time() }, 100_000_000);

    println!("TOTAL_VIOLATIONS={total_violations}");
}
