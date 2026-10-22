//! Per-task synchronous observations. Clones share counters; enter each worker explicitly.
use rustdx_complete::atomic_file::AtomicFile;
use serde::{Serialize, Serializer};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::File,
    io::{self, Write},
    marker::PhantomData,
    path::Path,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Source,
    Download,
    Extract,
    Snapshot,
    Parse,
    Sort,
    Write,
    Readback,
}

const STAGES: [Stage; 8] = [
    Stage::Source,
    Stage::Download,
    Stage::Extract,
    Stage::Snapshot,
    Stage::Parse,
    Stage::Sort,
    Stage::Write,
    Stage::Readback,
];

#[derive(Default)]
struct Timing {
    calls: u64,
    failures: u64,
    active: u64,
    accumulated: Duration,
    wall: Duration,
    active_since: Option<Instant>,
}

#[derive(Default)]
struct State {
    timings: BTreeMap<Stage, Timing>,
    temporary_current: u64,
    temporary_peak: u64,
}

struct Task {
    started: Instant,
    started_at: chrono::DateTime<chrono::Utc>,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct Observations(Arc<Task>);

thread_local! {
    static CURRENT: RefCell<Option<Observations>> = const { RefCell::new(None) };
}

impl Default for Observations {
    fn default() -> Self {
        Self(Arc::new(Task {
            started: Instant::now(),
            started_at: chrono::Utc::now(),
            state: Mutex::new(State::default()),
        }))
    }
}

impl Observations {
    fn state(&self) -> MutexGuard<'_, State> {
        // Observability must not cause a second panic during unwinding.
        self.0.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn current() -> Option<Self> {
        CURRENT.with(|slot| slot.borrow().clone())
    }

    pub fn started_at(&self) -> String {
        self.0.started_at.to_rfc3339()
    }

    pub fn enter(&self) -> EnterGuard {
        let previous = CURRENT.with(|slot| slot.replace(Some(self.clone())));
        EnterGuard {
            previous,
            _thread_bound: PhantomData,
        }
    }

    pub fn span(&self, stage: Stage) -> Span {
        let start = Instant::now();
        let mut state = self.state();
        let timing = state.timings.entry(stage).or_default();
        timing.calls += 1;
        if timing.active == 0 {
            timing.active_since = Some(start);
        }
        timing.active += 1;
        Span {
            observations: self.clone(),
            stage,
            start,
            success: false,
        }
    }

    fn temporary(&self) -> Arc<Mutex<TemporaryBytes>> {
        Arc::new(Mutex::new(TemporaryBytes {
            observations: self.clone(),
            bytes: 0,
            released: false,
        }))
    }
}

pub struct EnterGuard {
    previous: Option<Observations>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

/// An unfinished or unwound span is a failed invocation.
pub struct Span {
    observations: Observations,
    stage: Stage,
    start: Instant,
    success: bool,
}

impl Span {
    pub fn success(&mut self) {
        self.success = true;
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let now = Instant::now();
        let mut state = self.observations.state();
        let timing = state.timings.entry(self.stage).or_default();
        timing.accumulated += now.duration_since(self.start);
        timing.failures += u64::from(!self.success);
        timing.active -= 1;
        if timing.active == 0
            && let Some(start) = timing.active_since.take()
        {
            timing.wall += now.duration_since(start);
        }
    }
}

pub fn measure<T, E>(stage: Stage, run: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let mut span = Observations::current().map(|task| task.span(stage));
    let result = run();
    if result.is_ok()
        && let Some(span) = &mut span
    {
        span.success();
    }
    result
}

impl Serialize for Observations {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let now = Instant::now();
        let state = self.state();
        let stages: BTreeMap<_, _> = STAGES
            .into_iter()
            .map(|stage| {
                let timing = state.timings.get(&stage);
                let wall = timing.map_or(Duration::ZERO, |t| {
                    t.wall + t.active_since.map_or(Duration::ZERO, |s| now.duration_since(s))
                });
                (
                    stage,
                    serde_json::json!({
                        "calls": timing.map_or(0, |t| t.calls),
                        "failures": timing.map_or(0, |t| t.failures),
                        "in_flight": timing.map_or(0, |t| t.active),
                        "completed_span_ms": timing.map_or(0.0, |t| t.accumulated.as_secs_f64() * 1000.0),
                        "active_wall_ms": wall.as_secs_f64() * 1000.0,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "elapsed_wall_ms": now.duration_since(self.0.started).as_secs_f64() * 1000.0,
            "timing_scope": "inclusive_spans; completed_span_ms_sums_threads_and_nested_calls; active_wall_ms_is_per_stage_union; neither_is_cpu_time; stages_must_not_be_summed",
            "stages": stages,
            "temporary_current_bytes": state.temporary_current,
            "temporary_peak_bytes": state.temporary_peak,
            "temporary_scope": "successful_write_logical_bytes_of_day_snapshots_sorted_runs_spools_and_staged_outputs; excludes_zip_cache_extraction_manifests_evidence_and_subprocess_scratch; not_allocated_disk_blocks",
        })
        .serialize(serializer)
    }
}

struct TemporaryBytes {
    observations: Observations,
    bytes: u64,
    released: bool,
}

impl TemporaryBytes {
    fn add(&mut self, bytes: u64) {
        if !self.released {
            self.bytes += bytes;
            let mut state = self.observations.state();
            state.temporary_current += bytes;
            state.temporary_peak = state.temporary_peak.max(state.temporary_current);
        }
    }

    fn release(&mut self) {
        if !self.released {
            self.observations.state().temporary_current -= self.bytes;
            self.released = true;
        }
    }
}

/// Accounting follows the temporary path, including while queued between threads.
pub struct TemporaryFile {
    atomic: Option<AtomicFile>,
    bytes: Option<Arc<Mutex<TemporaryBytes>>>,
}

impl TemporaryFile {
    pub fn create(path: impl AsRef<Path>) -> io::Result<(Self, TrackedWriter)> {
        let (atomic, file) = AtomicFile::create(path)?;
        let bytes = Observations::current().map(|task| task.temporary());
        Ok((
            Self {
                atomic: Some(atomic),
                bytes: bytes.clone(),
            },
            TrackedWriter { file, bytes },
        ))
    }

    pub fn temporary_path(&self) -> &Path {
        self.atomic
            .as_ref()
            .expect("live temporary file")
            .temporary_path()
    }

    pub fn commit(mut self) -> io::Result<()> {
        self.atomic.take().expect("live temporary file").commit()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        // Unlink before releasing the logical accounting.
        drop(self.atomic.take());
        if let Some(bytes) = &self.bytes {
            bytes.lock().unwrap_or_else(|e| e.into_inner()).release();
        }
    }
}

pub struct TrackedWriter {
    file: File,
    bytes: Option<Arc<Mutex<TemporaryBytes>>>,
}

impl TrackedWriter {
    pub fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.file.metadata()
    }
}

impl Write for TrackedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.file.write(buffer)?;
        if let Some(bytes) = &self.bytes {
            bytes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .add(written as u64);
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_nested_spans_and_unwinding_keep_task_counters_separate() {
        let task = Observations::default();
        let other = Observations::default();
        let guard = task.enter();
        {
            let _guard = other.enter();
            measure(Stage::Download, || Ok::<_, ()>(())).unwrap();
        }
        let barrier = Arc::new(std::sync::Barrier::new(3));
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let barrier = barrier.clone();
                let task = task.clone();
                scope.spawn(move || {
                    let _guard = task.enter();
                    let mut outer = task.span(Stage::Parse);
                    let mut inner = task.span(Stage::Parse);
                    barrier.wait();
                    std::thread::sleep(Duration::from_millis(20));
                    inner.success();
                    outer.success();
                });
            }
            barrier.wait();
        });
        assert!(
            std::panic::catch_unwind(|| {
                let _failed = task.span(Stage::Write);
                panic!("simulated failure");
            })
            .is_err()
        );
        let json = serde_json::to_value(&task).unwrap();
        assert_eq!(json["stages"]["parse"]["calls"], 4);
        assert_eq!(json["stages"]["parse"]["in_flight"], 0);
        assert_eq!(json["stages"]["write"]["failures"], 1);
        assert_eq!(json["stages"]["download"]["calls"], 0);
        let parse = &json["stages"]["parse"];
        assert!(
            parse["completed_span_ms"].as_f64().unwrap()
                > parse["active_wall_ms"].as_f64().unwrap()
        );
        drop(guard);
        assert!(Observations::current().is_none());
        assert_eq!(
            serde_json::to_value(other).unwrap()["stages"]["download"]["calls"],
            1
        );
    }

    #[test]
    fn queued_temporary_bytes_release_on_drop_and_publication() {
        let task = Observations::default();
        let _guard = task.enter();
        let dir = std::env::temp_dir().join(format!("rustdx-observe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (first, mut writer) = TemporaryFile::create(dir.join("first.csv")).unwrap();
        writer.write_all(b"abc").unwrap();
        drop(writer);
        let (second, mut writer) = TemporaryFile::create(dir.join("second.csv")).unwrap();
        writer.write_all(b"hello").unwrap();
        drop(writer);
        let first_path = first.temporary_path().to_owned();
        std::thread::spawn(move || drop(first)).join().unwrap();
        assert!(!first_path.exists());
        assert_eq!(
            serde_json::to_value(&task).unwrap()["temporary_current_bytes"],
            5
        );
        second.commit().unwrap();
        let json = serde_json::to_value(task).unwrap();
        assert_eq!(json["temporary_current_bytes"], 0);
        assert_eq!(json["temporary_peak_bytes"], 8);
        assert_eq!(std::fs::read(dir.join("second.csv")).unwrap(), b"hello");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
