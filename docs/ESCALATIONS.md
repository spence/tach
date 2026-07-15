# ESCALATIONS — tach

Owner-only blockers, filed by unattended agents so they can move on. When an agent hits a decision only the owner can make, it files a row here and continues with other work — it does not block and wait. The owner reviews this queue on their next interaction.

- **Lifecycle:** 🟠 OPEN → 👁 ACKED → ✅ ACCEPTED / ⛔️ REJECTED / ☑️ RESOLVED. ACCEPTED and REJECTED are **owner-only**; an agent may self-☑️ RESOLVE only when the blocker genuinely disappears.

## Open queue

| ID | Status | Description | Blocks | Context |
|---|---|---|---|---|
| `ESC-HOSTED-EVIDENCE-PUSH` | ☑️ RESOLVED | Approve pushing bench/six-clock-speed and dispatching .github/workflows/bench-speed-windows.yml with boundary=release-missing to collect only native Windows x86_64 and macOS Intel evidence. | `OBJ-PROVE-TIMERS.M1.G1` | [entry](#esc-hosted-evidence-push) |
| `ESC-PUBLISH-TACH-0-2-0-76FD4B1` | 🟠 OPEN | Approve publishing tach 0.2.0 from candidate 76fd4b1 after reviewing the final README and retained approval packet. | `OBJ-RELEASE-0-2.M2.G1` | [entry](#esc-publish-tach-0-2-0-76fd4b1) |

## `ESC-HOSTED-EVIDENCE-PUSH`

- **Status:** ☑️ RESOLVED
- **Filed:** 2026-07-14 by unattended agent
- **Blocks gate:** `OBJ-PROVE-TIMERS.M1.G1`
- **Owner decision needed:** Approve pushing bench/six-clock-speed and dispatching .github/workflows/bench-speed-windows.yml with boundary=release-missing to collect only native Windows x86_64 and macOS Intel evidence.

- 2026-07-14 🟠 OPEN — filed; agent moved on to other work.
- 2026-07-14 ☑️ RESOLVED — the blocker genuinely disappeared.

## `ESC-PUBLISH-TACH-0-2-0-76FD4B1`

- **Status:** 🟠 OPEN
- **Filed:** 2026-07-14 by unattended agent
- **Blocks gate:** `OBJ-RELEASE-0-2.M2.G1`
- **Owner decision needed:** Approve publishing tach 0.2.0 from candidate 76fd4b1 after reviewing the final README and retained approval packet.

- 2026-07-14 🟠 OPEN — filed; agent moved on to other work.
