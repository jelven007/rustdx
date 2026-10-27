# File-cache owner release and Windows success-fixture budget

Status: local verification passed; remote PR checks pending
Mode: deep
Severity: blocks PR CI
Last updated: 2026-10-27

PR CI run [40528096534](https://github.com/jelven007/rustdx/actions/runs/40528096534)
failed twice on Windows cache concurrency tests. Both subprocess capacity
maintenance and atomic-replacement readers could exceed the unchanged
five-second directory-lock deadline.

The diagnostic run
[40532548329](https://github.com/jelven007/rustdx/actions/runs/40532548329)
passed both targeted cache tests and the full core library. Its 32KiB writes
took 8–12ms and initial readers waited 244–346ms. Its only failure was the
40-security CLI success fixture exhausting its synthetic 15-second operation
budget; the independent failure/deadline fixture passed.

| Hypothesis | Evidence and conclusion |
| --- | --- |
| Slow writes necessarily consume the five-second lock budget. | Not supported by the diagnostic timings: a full writer burst completed in less than one second. |
| Cache replacement or deletion has a separate Windows sharing error. | No sharing violation appeared in captured child output or the diagnostic run. This remains unobserved. |
| Closing the lock owner's file can leave ownership cleanup delayed. | Confirmed for duplicated Unix open file descriptions by three deterministic failing tests. Microsoft documents potentially delayed close-only unlocking on Windows. |
| The 40-security fixture's success budget is too small on shared Windows hosts. | The full Windows job exhausted precisely that fixture's overall operation budget while all its failure cases passed. |

`FileCache::lock` previously returned a raw `File`. The private
`FileCacheLock` now explicitly unlocks on drop before closing the file,
matching the existing CLI and runner cleanup pattern. All six file-cache
critical sections keep the guard alive for their full scope. Lock-file
identity, five-second acquisition deadline, operation cancellation and
capacity budgets remain unchanged. Drop is non-panicking; file closure
still follows if the OS reports an unlock error.

On Unix, `File::try_clone` shares the open file description, reproducing a
descriptor temporarily inherited by a concurrent fork. The regression
`file_lock_owner_drop_releases_while_duplicated_handle_remains_open`
asserts exclusivity, keeps that clone open, drops the owner and immediately
reacquires through a separately opened file. No fixed sleeps are used.

Microsoft's
[LockFileEx remarks](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex)
state that automatic unlocking on close can depend on system resources and
recommend explicit unlocking. The Unix regression proves the owner-release
defect; it does not directly reproduce Windows kernel cleanup timing.
Windows CI therefore repeats both original cache regressions three times
before running the full, parallel workspace gate. Temporary timing prints
were removed; captured subprocess failure output is retained.

Only the 40-security successful collection fixture gets a 60-second schedule
budget because it persists hundreds of archive files. The common 15-second
failure budget and independent 150ms parent-deadline assertion remain intact.
Production operation budgets are unchanged.

Verification already observed:

- Before the fix, the owner-release regression failed three consecutive
  times with `WouldBlock`.
- Explicit unlock made that regression pass.
- Removing only the drop implementation caused three more consecutive
  failures; the implementation was then restored.
- Full core checks passed: 488 passed, zero failed, 35 ignored under the CI
  offline configuration. Formatting, documentation navigation/lifecycle and
  version-policy/PR checks passed.
- All seven CLI TCP collection tests passed, including successful bounded
  40-security collection and the independent failure/deadline assertions.
  Core and CLI all-target/all-feature Clippy passed with warnings denied.
  Remote validation is pending.

Raw diagnostic, red, green and reverse-red logs are retained with the remote
publication evidence. No frozen protocol or acceptance evidence was changed.
