# Windows file-cache publication blocker

Status: root cause confirmed; queued Windows fix implemented; native runtime verification pending
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

## Buffered failure and confirmed cause

Run 40639204437 passes the three isolated contention repetitions but fails both
cache contention tests in the full parallel Windows suite; the other nine jobs
pass. The writer's 24 operations acquire immediately on their first attempt,
successfully unlock each time, and hold the lock for a cumulative 9.4725944s.
The longest individual hold is 1.2177736s. All four readers start within 0.512ms,
attempt acquisition 247 times, and time out at approximately 5.002s without
ever acquiring. The independent-process child also reports the same five-second
lock deadline. Raw job 122653133730 logs remain outside the repository.

This confirms repeated immediate reacquisition starving polling contenders
when full-suite disk contention makes durable writes slower. Individual holds
are below the lock deadline; successful explicit unlocks exclude retained
ownership in this captured failure. The isolated successes take approximately
0.4s and cannot qualify the full parallel workload.

A repository-external Rust 1.89 probe independently reproduces starvation in
three trials using 24 bounded 300ms owners, successful unlocks and the original
20ms/five-second polling policy. It did not itself identify the Windows cause;
the failed Windows buffered records establish that link.

## Fix and regression contract

`cache_windows_lock.rs` opens one native `LockFileEx` request without
`LOCKFILE_FAIL_IMMEDIATELY`, using an overlapped lock-file handle and a private
manual-reset event. `GetOverlappedResultEx` waits in at most 20ms slices while
checking the same five-second deadline and parent task cancellation. The byte
range and explicit successful-owner unlock remain compatible with other cache
instances and processes; no runtime or background thread is introduced.

The request owns stable `UnsafeCell<OVERLAPPED>` storage and its event while
borrowing the live file. Failure, cancellation or panic cancels only that
request and acknowledges native completion before freeing either resource.
If cancellation races successful acquisition, Drop explicitly releases the
acquired lock. Native driver cancellation acknowledgement cannot be forcibly
preempted, matching the documented underlying-I/O boundary.

Three Windows-only regressions use pending requests and ownership assertions,
without scheduler sleeps: queued ownership precedes owner reacquisition;
expired requests cancel while the owner remains locked; cancellation releases
a request completed concurrently. The original four readers, 100 reads,
24 complete 32KiB replacements, process-capacity test and five-second production
deadline are unchanged. All temporary diagnostics are removed.

Fresh local formatting, core all-target/all-feature Clippy and 202 core unit
tests pass. Windows cross-target all-target/all-feature Clippy passes after
correcting one needless return in the platform branch. Documentation navigation
(53 documents/1181 links), lifecycle (36 documents), Python validation and
unchanged 2.0.3 version/PR policy pass. Windows CI temporarily restores immediate
acquisition for three reverse-red handoff runs, requires the actual one-test
failure and Windows lock-violation code 33, restores exact source bytes in
finally, then repeats all three queue/cancellation regressions with the fix.
Native Windows tests, this reverse-red proof and complete PR/main release gates remain
required before publication. FR-12 and the registered integration contract
now trace the queue, cancellation tests and native-I/O boundary.
