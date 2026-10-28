# Windows file-cache publication blocker

Status: diagnostic evidence pending
Mode: deep
Severity: blocks release

PR runs 40629801584 and 40630873161 both fail the unchanged
`parallel_file_reads_observe_complete_atomic_replacements` regression:
readers reach the five-second directory-lock deadline. The latest run's
other nine gates pass. No release has been uploaded.

The explicit owner unlock from the previous investigation is already present.
The two current failures are retained in the local release report directory.

| Hypothesis | Prior / confidence | Predicted observation and evidence |
| --- | --- | --- |
| Windows durable writes are now slow enough that the writer burst exceeds five seconds. | 35% / medium | Operation timings show cumulative writes exceeding five seconds; readers fail while writes continue. |
| Unfair reacquisition starves readers despite individually short writes. | 30% / medium | Readers repeatedly wait while successful writer operations acquire and release the same lock. |
| OS lock ownership persists after explicit unlock. | 20% / low | Completed operations are followed by a long interval with no successful owner; lock-level diagnostics would then be required. |
| Shared CI scheduling pauses threads, rather than cache I/O causing the delay. | 15% / low | Large gaps appear between operations rather than in sustained write durations; additional hold/wait timing would distinguish this. |

Temporary tagged logs only instrument the existing regression. Its values,
four readers, 24 replacements, assertions and lock deadline are unchanged.
Production code is unchanged while gathering current Windows evidence.
