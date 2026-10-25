//! Bounded, durable TCP wire archives usable by CLI library callers and runner adapters.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rustdx_complete::tcp::{FrameEvent, FrameObserver, Tdx, decode_frame, decode_response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireRecord {
    pub schema_version: u32,
    pub phase: String,
    pub captured_at: String,
    pub endpoint: SocketAddr,
    pub request: Vec<u8>,
    pub header: Option<[u8; 16]>,
    pub body: Vec<u8>,
    pub error_kind: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    file: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    schema_version: u32,
    entries: Vec<Entry>,
}

#[derive(Debug)]
struct State {
    remaining: u64,
    max_records: usize,
    inventory: Inventory,
    finished: bool,
}

#[derive(Debug)]
pub struct FrameArchive {
    root: PathBuf,
    state: Mutex<State>,
}

fn write_new(root: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    // Directory handles can be synced on Unix; Windows rejects File::open on a directory.
    #[cfg(unix)]
    File::open(root)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = root;
    Ok(())
}

impl FrameArchive {
    /// Requires a new directory: no existing archive can be replaced accidentally.
    pub fn create(root: &Path, max_bytes: u64, max_records: usize) -> io::Result<Self> {
        if max_bytes == 0 || !(1..=100_000).contains(&max_records) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid archive budget",
            ));
        }
        fs::create_dir(root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            root: root.into(),
            state: Mutex::new(State {
                remaining: max_bytes,
                max_records,
                inventory: Inventory {
                    schema_version: 1,
                    entries: Vec::new(),
                },
                finished: false,
            }),
        })
    }

    /// Seal the digest inventory only after all acquisition threads have completed.
    pub fn finish(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("archive mutex poisoned"))?;
        if state.finished {
            return Err(io::Error::other("archive already finished"));
        }
        let bytes = serde_json::to_vec(&state.inventory)?;
        let available = state.remaining + state.inventory.entries.len() as u64 * 512;
        if bytes.len() as u64 > available {
            return Err(io::Error::other("archive budget exhausted"));
        }
        write_new(&self.root, &self.root.join("inventory.json"), &bytes)?;
        state.remaining = available - bytes.len() as u64;
        state.finished = true;
        Ok(())
    }
}

impl FrameObserver for FrameArchive {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        rustdx_complete::operation::check()?;
        let (phase, endpoint, request, header, body, error_kind) = match event {
            FrameEvent::Request { endpoint, request } => {
                ("request", endpoint, request, None, &[][..], None)
            }
            FrameEvent::Response {
                endpoint,
                request,
                header,
                body,
            } => ("response", endpoint, request, Some(*header), body, None),
            FrameEvent::Failed {
                endpoint,
                request,
                kind,
            } => (
                "failed",
                endpoint,
                request,
                None,
                &[][..],
                Some(format!("{kind:?}")),
            ),
        };
        if request.len() > 65535 || body.len() > 65535 {
            return Err(io::Error::other("wire record exceeds frame bound"));
        }
        let record = WireRecord {
            schema_version: 1,
            phase: phase.into(),
            captured_at: chrono::Utc::now().to_rfc3339(),
            endpoint,
            request: request.into(),
            header,
            body: body.into(),
            error_kind,
        };
        let bytes = serde_json::to_vec(&record)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("archive mutex poisoned"))?;
        let count = state.inventory.entries.len();
        // Reserve inventory bytes before writing, so success can always be sealed.
        if state.finished
            || count >= state.max_records
            || bytes.len() as u64 + 512 > state.remaining
        {
            return Err(io::Error::other("wire archive budget exhausted or closed"));
        }
        let file = format!("{count:06}.json");
        write_new(&self.root, &self.root.join(&file), &bytes)?;
        state.remaining -= bytes.len() as u64 + 512;
        state.inventory.entries.push(Entry {
            file,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
        });
        Ok(())
    }
}

pub fn load_response(root: &Path, sequence: usize) -> io::Result<WireRecord> {
    let limited = |path: &Path, limit: u64| -> io::Result<Vec<u8>> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(io::Error::other("archive symlink rejected"));
        }
        let mut bytes = Vec::new();
        File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(io::Error::other("archive file limit exceeded"));
        }
        Ok(bytes)
    };
    let inventory: Inventory =
        serde_json::from_slice(&limited(&root.join("inventory.json"), 32 * 1024 * 1024)?)?;
    if inventory.schema_version != 1 || inventory.entries.len() > 100_000 {
        return Err(io::Error::other("invalid wire archive inventory"));
    }
    let entry = inventory
        .entries
        .get(sequence)
        .ok_or_else(|| io::Error::other("record absent"))?;
    if entry.file != format!("{sequence:06}.json") {
        return Err(io::Error::other("invalid record path"));
    }
    let bytes = limited(&root.join(&entry.file), 1024 * 1024)?;
    if bytes.len() as u64 != entry.bytes || format!("{:x}", Sha256::digest(&bytes)) != entry.sha256
    {
        return Err(io::Error::other("wire record differs from inventory"));
    }
    let record: WireRecord = serde_json::from_slice(&bytes)?;
    if record.schema_version != 1 || record.phase != "response" || record.request.len() > 65535 {
        return Err(io::Error::other("record is not a complete response"));
    }
    decode_frame(
        &record
            .header
            .ok_or_else(|| io::Error::other("missing wire header"))?,
        &record.body,
        false,
    )?;
    Ok(record)
}

/// Verify and visit an entire sealed archive in wire order, retaining at most
/// one record body. Requests and failures are included, so consumers can check
/// pairing and cannot silently skip rejected frames. The inventory is read once.
pub fn visit_records(
    root: &Path,
    max_bytes: u64,
    max_records: usize,
    mut visitor: impl FnMut(usize, WireRecord) -> io::Result<()>,
) -> io::Result<()> {
    fn limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        if !fs::symlink_metadata(path)?.is_file() {
            return Err(io::Error::other("archive requires regular files"));
        }
        let mut bytes = Vec::new();
        File::open(path)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(io::Error::other("archive file limit exceeded"));
        }
        Ok(bytes)
    }
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(io::Error::other("archive requires a regular directory"));
    }
    let bytes = limited(
        &root.join("inventory.json"),
        max_bytes.min(32 * 1024 * 1024),
    )?;
    let mut total = bytes.len() as u64;
    let inventory: Inventory = serde_json::from_slice(&bytes)?;
    if inventory.schema_version != 1
        || inventory.entries.is_empty()
        || inventory.entries.len() > max_records.min(100_000)
    {
        return Err(io::Error::other("invalid wire archive inventory"));
    }
    let mut files = 0;
    for entry in fs::read_dir(root)? {
        rustdx_complete::operation::check()?;
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name != "inventory.json"
            && !(name.len() == 11
                && name.is_ascii()
                && name.ends_with(".json")
                && name[..6].bytes().all(|b| b.is_ascii_digit())
                && name[..6]
                    .parse::<usize>()
                    .is_ok_and(|i| i < inventory.entries.len()))
        {
            return Err(io::Error::other("file outside wire archive inventory"));
        }
        files += 1;
    }
    if files != inventory.entries.len() + 1 {
        return Err(io::Error::other("wire archive inventory is incomplete"));
    }
    for (sequence, entry) in inventory.entries.into_iter().enumerate() {
        rustdx_complete::operation::check()?;
        if entry.file != format!("{sequence:06}.json") {
            return Err(io::Error::other("invalid record path"));
        }
        let bytes = limited(
            &root.join(&entry.file),
            max_bytes.saturating_sub(total).min(1024 * 1024),
        )?;
        total += bytes.len() as u64;
        if bytes.len() as u64 != entry.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != entry.sha256
        {
            return Err(io::Error::other("wire record differs from inventory"));
        }
        let record: WireRecord = serde_json::from_slice(&bytes)?;
        if record.schema_version != 1 || record.request.len() > 65535 || record.body.len() > 65535 {
            return Err(io::Error::other("invalid wire record"));
        }
        visitor(sequence, record)?;
    }
    Ok(())
}

/// Replay archived bytes through the real checked parser. No server is contacted.
pub fn replay<T: Tdx>(record: &WireRecord, parser: &mut T) -> io::Result<()> {
    if parser.send() != record.request {
        return Err(io::Error::other("replay request differs from parser"));
    }
    parser.try_parse(decode_response(
        &record.request,
        &record
            .header
            .ok_or_else(|| io::Error::other("missing wire header"))?,
        &record.body,
        true,
    )?)
}
