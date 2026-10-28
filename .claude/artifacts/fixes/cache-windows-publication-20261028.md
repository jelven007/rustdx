# Windows file-cache publication blocker

Status: uninstrumented failure reproduced; buffered lock-level diagnostics pending
Mode: deep
Severity: blocks release

PR runs 40629801584 and 40630873161 both fail the unchanged
`parallel_file_reads_observe_complete_atomic_replacements` regression:
readers reach the five-second directory-lock deadline. The latest run's
other nine gates pass. No release has been uploaded.

The explicit owner unlock from the previous investigation is already present.
The two current failures are retained outside the source workspace in the
publication evidence directory.

| Hypothesis | Prior / confidence | Predicted observation and evidence |
| --- | --- | --- |
| Windows durable writes are now slow enough that the writer burst exceeds five seconds. | 35% / medium | Operation timings show cumulative writes exceeding five seconds; readers fail while writes continue. |
| Unfair reacquisition starves readers despite individually short writes. | 30% / medium | Readers repeatedly wait while successful writer operations acquire and release the same lock. |
| OS lock ownership persists after explicit unlock. | 20% / low | Completed operations are followed by a long interval with no successful owner; lock-level diagnostics would then be required. |
| Shared CI scheduling pauses threads, rather than cache I/O causing the delay. | 15% / low | Large gaps appear between operations rather than in sustained write durations; additional hold/wait timing would distinguish this. |

Temporary tagged logs only instrument the existing regression. Its values,
four readers, 24 replacements, assertions and lock deadline are unchanged.
Production code is unchanged while gathering current Windows evidence.

Run 40632374247 passed all ten gates, including three repetitions of both
Windows cache regressions and the full Windows suite. The 24-write bursts
took 333.99ms, 358.41ms and 303.67ms; initial reader waits were at most
386.18ms. No sharing violation or unlock failure was observed.

These successful samples do not identify the cause of the earlier failures:
slow I/O and scheduling remain inconclusive for those failed hosts, and
the current samples do not prove five-second starvation or deferred ownership.
No production fix is justified by this evidence. Temporary instrumentation
is removed, restoring `cache.rs` byte-for-byte to the pre-diagnostic source.
The original values, assertions, repetition count and lock deadline remain.
Fresh uninstrumented PR and main gates are required before publication.

The uninstrumented run 40633683998 again fails with all four readers reaching
the unchanged five-second lock deadline; the test lasts 8.20 seconds.
The other nine gates, including real isolated database acceptance, pass.
This rules out treating the earlier instrumented success as release clearance.

The next diagnostic records lock wait, hold duration, attempt count and unlock
errors in thread-local memory. Each worker buffers operation results; logging
occurs only after all five workers join, avoiding hot-path console scheduling.
The four readers, 100 reads each, 24 replacements, full byte assertions and
five-second deadline remain. Production builds have no timing instrumentation.
Local Rust 1.89 formatting, core all-target/all-feature Clippy and the original
complete-value regression pass before pushing this diagnostic.
