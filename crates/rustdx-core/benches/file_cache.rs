//! Fixed real response bytes replicated across keys; not distinct market observations.
use rustdx_complete::cache::{FileCache, stable_key};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::Barrier,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("RUSTDX_BENCH_ROOT")?);
    if root.exists() {
        return Err("benchmark root must be new".into());
    }
    fs::create_dir(&root)?;
    let payload = include_bytes!("../tests/fixtures/daily-shares/115.238.56.198-sh600000.wire.bin");
    let expiry = u64::try_from(
        (SystemTime::now().duration_since(UNIX_EPOCH)? + Duration::from_secs(86400)).as_millis(),
    )?;
    let mut samples = Vec::new();
    for entries in [1000, 10000] {
        let dir = root.join(entries.to_string());
        let cache = FileCache::try_with_capacity(&dir, entries + 1, 64 * 1024 * 1024)?;
        // Fixture setup is outside timing. Use the public on-disk format to
        // avoid O(N²) setup masking the per-operation measurement.
        for index in 0..entries {
            let key = format!("replayed-raw-{index}");
            let mut file = fs::File::create(dir.join(format!("{}.cache", stable_key(&key))))?;
            file.write_all(b"RDXC0002")?;
            file.write_all(&expiry.to_le_bytes())?;
            file.write_all(&(key.len() as u32).to_le_bytes())?;
            file.write_all(key.as_bytes())?;
            file.write_all(payload)?;
        }
        for workers in [1, 4, 8] {
            for round in 0..5 {
                // Write scans are measured singly; the retained earlier stress
                // run separately records starvation with concurrent writers.
                for (action, operations) in [("hit", 128), ("overwrite", 1)] {
                    let barrier = Barrier::new(workers);
                    let start = Instant::now();
                    let outcomes =
                        std::thread::scope(|scope| -> io::Result<Vec<(u128, Option<String>)>> {
                            let handles: Vec<_> = (0..workers)
                                .map(|worker| {
                                    let cache = &cache;
                                    let barrier = &barrier;
                                    scope.spawn(
                                        move || -> io::Result<Vec<(u128, Option<String>)>> {
                                            barrier.wait();
                                            let mut times = Vec::new();
                                            for index in (worker..operations).step_by(workers) {
                                                let key =
                                                    format!("replayed-raw-{}", index % entries);
                                                let start = Instant::now();
                                                let result = if action == "hit" {
                                                    cache.try_get(&key).and_then(|value| {
                                                        if value.as_deref()
                                                            == Some(payload.as_slice())
                                                        {
                                                            Ok(())
                                                        } else {
                                                            Err(io::Error::other(
                                                                "cached raw bytes differ",
                                                            ))
                                                        }
                                                    })
                                                } else {
                                                    cache.try_set(
                                                        &key,
                                                        payload,
                                                        Duration::from_secs(86400),
                                                    )
                                                };
                                                times.push((
                                                    start.elapsed().as_nanos(),
                                                    result.err().map(|e| e.to_string()),
                                                ));
                                            }
                                            Ok(times)
                                        },
                                    )
                                })
                                .collect();
                            let mut times = Vec::new();
                            for handle in handles {
                                times.extend(
                                    handle.join().map_err(|_| {
                                        io::Error::other("benchmark thread panic")
                                    })??,
                                );
                            }
                            Ok(times)
                        })?;
                    let wall = start.elapsed().as_secs_f64();
                    let errors: Vec<_> = outcomes.iter().filter_map(|(_, e)| e.as_ref()).collect();
                    let mut durations: Vec<_> = outcomes.iter().map(|(ns, _)| *ns).collect();
                    durations.sort_unstable();
                    let usage = cache.usage()?;
                    if usage.entries != entries {
                        return Err("entry budget or overwrite behavior changed".into());
                    }
                    samples.push(json!({
                        "entries":entries,"workers":workers,"round":round,"action":action,
                        "effective_workers":if action == "hit" { workers } else { 1 },
                        "operations":operations,"wall_seconds":wall,
                        "errors":errors,
                        "operation_median_ns":durations[durations.len()/2],
                        "operation_p95_ns":durations[(durations.len()*95/100).min(durations.len()-1)],
                        "committed_bytes":usage.bytes,
                    }));
                    eprintln!(
                        "entries={entries} workers={workers} round={round} action={action} seconds={wall:.3} errors={}",
                        errors.len()
                    );
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version":1,"kind":"replicated_real_payload_cache",
            "payload_sha256":format!("{:x}",Sha256::digest(payload)),
            "payload_source":"tests/fixtures/daily-shares/115.238.56.198-sh600000.wire.bin",
            "scope":"Same-host cache latency including lock wait; replicated keys, not market throughput. Setup excluded. No syscall tracer.",
            "samples":samples
        }))?
    );
    Ok(())
}
