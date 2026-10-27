# Controller lock must release before inherited handles close

Status: FIXED
Mode: default
Severity: blocks PR CI
Last updated: 2026-10-27

The Ubuntu job of PR CI run
[40525426726](https://github.com/jelven007/rustdx/actions/runs/40525426726)
failed in `all_explicitly_exempt_keys_publish_empty_partitions_without_a_collector`.
Publication succeeded, but the subsequent `Ledger::open` failed because
`controller.lock` remained locked after `pipeline::execute` returned.

The owner must retain exclusive controller access throughout its scope and
release it when that scope ends, even if another thread is concurrently
spawning a process.

| Hypothesis | Diagnosis |
| --- | --- |
| Another test shares the same temporary root. | Rejected as the necessary cause. A single isolated regression with a fresh temporary directory fails three times without another controller or test. |
| SQLite retains its write lock after publication. | Rejected. The reported failure comes from `RootLock::acquire` before SQLite opens; the regression reproduces it without any database. |
| Closing the owner's file leaves its lock held by a duplicated open file description. | Confirmed. Keeping `File::try_clone` open reproduces the exact would-block error; explicit unlock before closing the owner removes it. |

On Unix, a concurrent fork shares open file descriptions until exec closes
close-on-exec descriptors. `RootLock` previously relied only on closing its
file. A duplicated or temporarily inherited descriptor can therefore retain
the lock after the controller scope ends. `pipeline::execute` correctly drops
its local guard; the missing explicit unlock is in that guard's cleanup.

`RootLock::drop` now explicitly unlocks the owned file before normal file
destruction, following the existing CLI lock-guard pattern. Cleanup remains
non-panicking and closes the file even if unlock reports an OS error.
The lock file is retained, so all controllers still lock the same inode.

The Unix storage regression
`dropping_root_lock_releases_it_while_a_duplicated_handle_remains_open`
keeps a cloned handle alive, drops the owner and immediately reacquires.
It also verifies exclusivity before owner drop, while the replacement owner
is alive and after the obsolete handle closes. It uses no fixed sleeps,
forks, external services or weakened lock assertions.

Verification observed in this task:

- V-1: regression before the fix failed three consecutive times with the
  same controller-lock would-block error.
- V-2: adding explicit unlock made the regression pass.
- V-3: temporarily removing only the cleanup implementation made the same
  test fail three more times; the fix was restored.
- V-4: full runner tests with all features and CI's
  `RUSTDX_SKIP_INTEGRATION_TESTS=1` passed: 38 passed, zero failed, 9 ignored,
  including the original exempt-publication regression.
- V-5: runner all-target/all-feature Clippy with warnings denied, formatting
  and whitespace checks passed.
- V-6: remote checks for the follow-up commit remain pending.

Raw red, green, reverse-red, runner-test and Clippy logs are retained in the
remote-publication evidence directory.

Pattern analysis: `git grep -n -e '\.unlock()' -e '_file: File' -- apps crates`
finds explicit cleanup in CLI `CacheLock`, `OutputLocks` and `TargetLock`.
The active runner `RootLock` was the close-only guard in that search.
Historical source snapshots and frozen acceptance evidence remain unchanged.
