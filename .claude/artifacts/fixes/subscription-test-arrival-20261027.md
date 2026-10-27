# Subscription regression must establish socket arrival

Status: FIXED
Mode: quick
Severity: blocks PR CI
Last updated: 2026-10-27

The PR CI run [40522780766](https://github.com/jelven007/rustdx/actions/runs/40522780766)
failed on both Ubuntu stable and Rust 1.89. The first failing test was
`partial_push_received_before_refresh_cannot_satisfy_the_new_request`:
the requested refresh returned cursor 11 instead of 12. The remaining
subscription failures were poisoned-lock consequences of that assertion.

The expected contract excludes a frame already partly received before a new
request from satisfying that request, even if the message identifiers coincide.

| Hypothesis | Diagnosis |
| --- | --- |
| The fixture's server-write notification proves client receipt. | Rejected. The notification follows `write_all`, which can leave the short prefix buffered in TCP. A 10 ms poll can therefore return before receiving it. |
| The production guard fails despite a staged prefix. | Not supported by the original local pass; deliberately removing the guard after synchronizing receipt causes the precise cursor assertion to fail three times. |

The test now clones the client socket solely for a bounded `peek` before its
short poll. This waits for actual readable bytes without consuming them or
altering the production API. The server withholds the rest of the frame until
the refresh request, so the poll stages a partial pre-request frame. The original
cursor, message identifier and synchronized-state assertions remain unchanged.

Verification observed in this task:

- Initial remote RED: both Ubuntu stable and MSRV show `Some(11)` versus `Some(12)`.
- Local synchronized test GREEN, followed by 100 consecutive successful runs.
- Temporarily disabling `pre_request_frame` produces three consecutive RED runs
  with the same cursor assertion; the production source was then restored.
- Formatting and whitespace checks passed.
- All-target/all-feature core-crate Clippy passed with warnings denied.
- Full core-crate tests with CI's `RUSTDX_SKIP_INTEGRATION_TESTS=1` passed:
  487 passed, zero failed and 35 ignored across 48 targets, including doctests.
- Fresh remote CI for the follow-up commit remains pending.

Raw logs are retained in the remote-publication evidence directory.
The first local full-suite command omitted the CI environment switch and
triggered external connection failures and a deadline assertion. That failed
attempt remains recorded separately; only the subsequent CI-configured run
is counted as passing verification.

Pattern search: `git grep -n -e 'sent_rx.recv' -e 'from_millis(10)' --
crates/rustdx-core/tests/quote_subscription.rs` found related readiness waits.
The other partial-frame tests keep distinct message identifiers or explicit
server gates; the new-request identifier collision is specific to this test.
This fix does not change those tests or production synchronization behavior.
