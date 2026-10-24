//! Forecast evidence, proof and readback budgets before any database transmission.
use serde::Serialize;

use crate::{Result, config::MAX_JSON, invalid, storage};

pub(super) const BATCH: usize = 512;
pub(super) const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;
pub(super) const IO_LIMIT: u64 = 50_000;
const IO_OVERHEAD: u64 = 4096;
const PROOF_IO_BYTES: u64 = 1536;

#[derive(Debug, Serialize)]
pub(super) struct Capacity {
    rows: u64,
    batches: u64,
    operations: u64,
    pub(super) evidence_bytes: u64,
    pub(super) evidence_entries: u64,
    proof_bytes: u64,
}

impl Capacity {
    pub(super) fn estimate(
        rows: u64,
        row_bytes: u64,
        partition_batches: u64,
        partitions: u64,
    ) -> Result<Self> {
        let batches = rows.div_ceil(BATCH as u64);
        // Initial inserts, verification, one recovery verification and one
        // complete consumer readback, with preflight/count/terminal queries.
        let operations = batches
            .checked_mul(3)
            .and_then(|v| v.checked_add(partition_batches))
            .and_then(|v| v.checked_add(partitions.checked_mul(2)?))
            .and_then(|v| v.checked_add(8))
            .ok_or_else(|| invalid("database operation forecast overflow"))?;
        let proof_bytes = operations
            .checked_mul(PROOF_IO_BYTES)
            .and_then(|v| v.checked_add(IO_OVERHEAD))
            .ok_or_else(|| invalid("database proof forecast overflow"))?;
        let evidence_bytes = row_bytes
            .checked_mul(19)
            .and_then(|v| v.checked_add(operations.checked_mul(IO_OVERHEAD)?))
            .and_then(|v| v.checked_add(proof_bytes.checked_mul(2)?))
            // A single full bounded error envelope may need more than a normal batch.
            .and_then(|v| v.checked_add(RESPONSE_LIMIT * 7 + IO_OVERHEAD))
            .ok_or_else(|| invalid("database evidence forecast overflow"))?;
        Ok(Self {
            rows,
            batches,
            operations,
            evidence_bytes,
            evidence_entries: operations * 2 + 4,
            proof_bytes,
        })
    }

    pub(super) fn check(
        &self,
        used_io: u64,
        usage: storage::TreeUsage,
        max_bytes: u64,
    ) -> Result<()> {
        if used_io.saturating_add(self.operations) > IO_LIMIT {
            return Err(invalid(
                "database capacity: I/O budget cannot cover publication and reserved readbacks",
            ));
        }
        if self
            .proof_bytes
            .saturating_add(used_io.saturating_mul(PROOF_IO_BYTES))
            > MAX_JSON
        {
            return Err(invalid(
                "database capacity: verification proof exceeds JSON byte limit",
            ));
        }
        if usage.entries.saturating_add(self.evidence_entries) > storage::MAX_TREE_ENTRIES {
            return Err(invalid(
                "database capacity: evidence entry budget cannot cover reserved readbacks",
            ));
        }
        if usage.bytes.saturating_add(self.evidence_bytes) > max_bytes {
            return Err(invalid(
                "database capacity: evidence byte budget cannot cover reserved readbacks",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_rejects_io_proof_entries_and_bytes_before_transmission() {
        let small = Capacity::estimate(1200, 400_000, 4, 2).unwrap();
        assert_eq!(small.batches, 3);
        assert_eq!(small.operations, 25);
        assert!(
            small
                .check(0, storage::TreeUsage::default(), 256 * 1024 * 1024)
                .is_ok()
        );
        assert!(
            small
                .check(IO_LIMIT - 1, storage::TreeUsage::default(), u64::MAX)
                .is_err()
        );
        assert!(
            small
                .check(
                    0,
                    storage::TreeUsage {
                        entries: storage::MAX_TREE_ENTRIES - 1,
                        bytes: 0,
                    },
                    u64::MAX
                )
                .is_err()
        );
        assert!(
            small
                .check(0, storage::TreeUsage::default(), small.evidence_bytes - 1)
                .is_err()
        );
        // The former INSERT-only upper bound cannot fit proof/recovery budgets.
        assert!(
            Capacity::estimate(12_798_976, 0, 24_998, 1)
                .unwrap()
                .check(0, storage::TreeUsage::default(), u64::MAX)
                .is_err()
        );
        // Evidence from all targets shares the same release tree.
        let used = storage::TreeUsage {
            bytes: small.evidence_bytes,
            entries: small.evidence_entries,
        };
        assert!(small.check(0, used, 2 * small.evidence_bytes - 1).is_err());
        assert!(Capacity::estimate(u64::MAX, u64::MAX, 1, 1).is_err());
        // Per-day export opens a session per partition, including empty days.
        assert!(
            Capacity::estimate(0, 0, 0, 30_000)
                .unwrap()
                .check(0, storage::TreeUsage::default(), u64::MAX)
                .is_err()
        );
    }
}
