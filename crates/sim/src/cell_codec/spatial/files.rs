//! Native spatial records. A small head advances only after all changed
//! records have reached disk. Unpublished generations are invisible to reads.
use super::{
    Acknowledgement, CANYON_INDEX_LIMIT, CANYON_RECORD_LIMIT, CanyonIndex, Capture,
    SALVAGE_RECORD_LIMIT, SalvageIndex, decode_canyon, decode_salvage,
};
use crate::{
    cell_codec::{Restored, paged},
    memory::MemoryChunk,
    underwater::SleepingCut,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAGIC: &[u8; 8] = b"CRABSP03";
const RECORD_LIMIT: usize = 8 * 1024 * 1024;
const PAGE_LIMIT: usize = 256 * 1024;
static TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Head {
    pub canyon_hash: Option<[u8; 32]>,
    pub epoch: u64,
    pub revision: u64,
    pub active_hash: [u8; 32],
}

pub struct Store {
    root: PathBuf,
}
#[derive(Serialize, Deserialize)]
struct Intent {
    epoch: u64,
    revision: u64,
    keys: Vec<(String, (i32, i32))>,
}
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn head(&self) -> Result<Option<Head>, String> {
        let Some(bytes) = read_optional(&self.root.join("head"), 128)? else {
            return Ok(None);
        };
        if !bytes.starts_with(MAGIC) {
            return Err("unsupported spatial save head".into());
        }
        postcard::from_bytes(&bytes[8..]).map(Some).map_err(issue)
    }
    pub fn commit(&self, capture: Capture, epoch: u64) -> Result<Acknowledgement, String> {
        self.commit_checked(capture, epoch, || true)
    }
    /// Recheck reset/session validity both before work and immediately before
    /// publishing the head. The caller serializes writers for this store.
    pub fn commit_checked(
        &self,
        capture: Capture,
        epoch: u64,
        current: impl Fn() -> bool,
    ) -> Result<Acknowledgement, String> {
        let expected = self.head()?.map(|h| (h.epoch, h.revision));
        self.commit_at(capture, epoch, expected, current)
    }
    fn lock(&self) -> Result<std::fs::File, String> {
        std::fs::create_dir_all(&self.root).map_err(issue)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.root.join(".lock"))
            .map_err(issue)?;
        file.lock().map_err(issue)?;
        Ok(file)
    }
    /// Replace a retired envelope with an empty new epoch, without reading or
    /// converting its world records. Damaged current-format heads still fail.
    pub fn retire_obsolete(&self) -> Result<bool, String> {
        let _lock = self.lock()?;
        let Some(bytes) = read_optional(&self.root.join("head"), 128)? else {
            return Ok(false);
        };
        if !bytes.starts_with(b"CRABSP01") && !bytes.starts_with(b"CRABSP02") {
            return Ok(false);
        }
        let (old_epoch, _, _): (u64, u64, [u8; 32]) =
            postcard::from_bytes(&bytes[8..]).map_err(issue)?;
        let head = Head {
            canyon_hash: None,
            epoch: old_epoch.checked_add(1).ok_or("save epoch exhausted")?,
            revision: 0,
            active_hash: [0; 32],
        };
        let mut bytes = MAGIC.to_vec();
        bytes.extend(postcard::to_allocvec(&head).map_err(issue)?);
        atomic(&self.root.join("head"), &bytes)?;
        let _ = std::fs::remove_dir_all(self.root.join("epochs"));
        let _ = std::fs::remove_file(self.root.join("pending"));
        Ok(true)
    }
    pub fn reset(&self) -> Result<(), String> {
        let _lock = self.lock()?;
        let epoch = self
            .head()?
            .map_or(Ok(1), |h| {
                h.epoch.checked_add(1).ok_or("save epoch exhausted")
            })
            .map_err(str::to_owned)?;
        let head = Head {
            canyon_hash: None,
            epoch,
            revision: 0,
            active_hash: [0; 32],
        };
        let mut bytes = MAGIC.to_vec();
        bytes.extend(postcard::to_allocvec(&head).map_err(issue)?);
        atomic(&self.root.join("head"), &bytes)?;
        let _ = std::fs::remove_dir_all(self.root.join("epochs"));
        let _ = std::fs::remove_file(self.root.join("pending"));
        Ok(())
    }
    pub fn commit_at(
        &self,
        capture: Capture,
        epoch: u64,
        expected: Option<(u64, u64)>,
        current: impl Fn() -> bool,
    ) -> Result<Acknowledgement, String> {
        let _lock = self.lock()?;
        if !current() {
            return Err("spatial save canceled".into());
        }
        if self.head()?.map(|h| (h.epoch, h.revision)) != expected {
            return Err("spatial save head changed in another process".into());
        }
        self.recover()?;
        let before = self.head()?;
        if before.is_some_and(|h| h.epoch != epoch) {
            return Err("stale spatial save epoch".into());
        }
        let revision = before
            .map_or(Ok(1), |h| {
                h.revision.checked_add(1).ok_or("save revision exhausted")
            })
            .map_err(str::to_owned)?;
        let mut canyon_writes = Vec::new();
        let mut canyon_remove = Vec::new();
        let mut salvage_writes = Vec::new();
        let mut salvage_remove = Vec::new();
        if let Some(records) = &capture.salvage {
            let index = SalvageIndex::new(records).map_err(issue)?;
            let previous = before
                .filter(|head| head.revision > 0)
                .map(|head| self.salvage_index(head))
                .transpose()?
                .flatten();
            if let Some(previous) = previous {
                for key in previous.keys().map_err(issue)? {
                    if !index.keys().map_err(issue)?.contains(key) {
                        salvage_remove.push(*key);
                    }
                }
            }
            for record in records {
                let raw = postcard::to_allocvec(record).map_err(issue)?;
                let old = before
                    .filter(|head| head.revision > 0)
                    .map(|head| self.page("salvage", record.region_id, head))
                    .transpose()?
                    .flatten();
                if old.as_deref() != Some(raw.as_slice()) {
                    salvage_writes.push(("salvage", record.region_id, raw));
                }
            }
            let raw = postcard::to_allocvec(&index).map_err(issue)?;
            let old = before
                .filter(|head| head.revision > 0)
                .map(|head| self.page("salvage_index", (0, 0), head))
                .transpose()?
                .flatten();
            if old.as_deref() != Some(raw.as_slice()) {
                salvage_writes.push(("salvage_index", (0, 0), raw));
            }
        }
        if let Some(records) = &capture.canyon {
            let index = CanyonIndex::new(records).map_err(issue)?;
            let previous = before
                .filter(|head| head.revision > 0)
                .map(|head| self.canyon_index(head))
                .transpose()?
                .flatten();
            if let Some(previous) = previous {
                for key in previous.keys().map_err(issue)? {
                    if !index.keys().map_err(issue)?.contains(key) {
                        canyon_remove.push(*key);
                    }
                }
            }
            for record in records {
                let raw = postcard::to_allocvec(record).map_err(issue)?;
                let old = before
                    .filter(|head| head.revision > 0)
                    .map(|head| self.page("canyon", record.region_id, head))
                    .transpose()?
                    .flatten();
                if old.as_deref() != Some(raw.as_slice()) {
                    canyon_writes.push(("canyon", record.region_id, raw));
                }
            }
            let raw = postcard::to_allocvec(&index).map_err(issue)?;
            let old = before
                .filter(|head| head.revision > 0)
                .map(|head| self.page("canyon_index", (0, 0), head))
                .transpose()?
                .flatten();
            if old.as_deref() != Some(raw.as_slice()) {
                canyon_writes.push(("canyon_index", (0, 0), raw));
            }
        }
        let committed_player = serde_json::from_slice(&capture.active.player).map_err(issue)?;
        let active = capture.active.encode().map_err(issue)?;
        let mut keys = vec![("active".to_owned(), (0, 0))];
        keys.extend(
            capture
                .sleeping
                .iter()
                .map(|cut| ("terrain".to_owned(), cut.key)),
        );
        keys.extend(
            capture
                .memory
                .iter()
                .map(|page| ("memory".to_owned(), (page.x as i32, page.y as i32))),
        );
        keys.extend(
            salvage_writes
                .iter()
                .map(|(kind, key, _)| (kind.to_string(), *key)),
        );
        keys.extend(
            salvage_remove
                .iter()
                .map(|key| ("salvage".to_owned(), *key)),
        );
        keys.extend(
            canyon_writes
                .iter()
                .map(|(kind, key, _)| (kind.to_string(), *key)),
        );
        keys.extend(canyon_remove.iter().map(|key| ("canyon".to_owned(), *key)));
        let intent = Intent {
            epoch,
            revision,
            keys,
        };
        atomic(
            &self.root.join("pending"),
            &postcard::to_allocvec(&intent).map_err(issue)?,
        )?;
        self.put(epoch, "active", (0, 0), revision, &active, RECORD_LIMIT)?;
        for (kind, key, raw) in &salvage_writes {
            self.put(epoch, kind, *key, revision, raw, SALVAGE_RECORD_LIMIT)?;
        }

        for (kind, key, raw) in &canyon_writes {
            self.put(epoch, kind, *key, revision, raw, CANYON_RECORD_LIMIT)?;
        }

        for cut in &capture.sleeping {
            self.put(
                epoch,
                "terrain",
                cut.key,
                revision,
                &cut.encode().map_err(issue)?,
                PAGE_LIMIT,
            )?;
        }
        for page in &capture.memory {
            let bytes = postcard::to_allocvec(page).map_err(issue)?;
            self.put(
                epoch,
                "memory",
                (page.x as i32, page.y as i32),
                revision,
                &bytes,
                PAGE_LIMIT,
            )?;
        }
        if !current() {
            return Err("spatial save canceled".into());
        }
        // Concurrent/expired writers cannot advance a head they did not capture.
        if self.head()?.map(|h| (h.epoch, h.revision)) != before.map(|h| (h.epoch, h.revision)) {
            return Err("spatial save head changed".into());
        }
        let canyon_hash = if let Some(records) = &capture.canyon {
            Some(paged::hash(
                &postcard::to_allocvec(&CanyonIndex::new(records).map_err(issue)?)
                    .map_err(issue)?,
            ))
        } else {
            before.and_then(|head| head.canyon_hash)
        };
        let head = Head {
            canyon_hash,
            epoch,
            revision,
            active_hash: paged::hash(&active),
        };
        let mut bytes = MAGIC.to_vec();
        bytes.extend(postcard::to_allocvec(&head).map_err(issue)?);
        atomic(&self.root.join("head"), &bytes)?;
        // Readers validate the current head before publishing. Once the new
        // head is durable, older coordinate versions can be reclaimed.
        for (kind, key) in &intent.keys {
            if let Ok(entries) = std::fs::read_dir(self.directory(epoch, kind, *key)) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy() != revision.to_string() {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
        for key in salvage_remove {
            let _ = std::fs::remove_dir_all(self.directory(epoch, "salvage", key));
        }
        for key in canyon_remove {
            let _ = std::fs::remove_dir_all(self.directory(epoch, "canyon", key));
        }
        let _ = std::fs::remove_file(self.root.join("pending"));
        Ok(Acknowledgement {
            canyon: capture.canyon,
            player: Some(committed_player),
            salvage: capture.salvage,
            sleeping: capture.sleeping,
            memory: capture.memory,
        })
    }
    fn recover(&self) -> Result<(), String> {
        for entry in std::fs::read_dir(&self.root).map_err(issue)? {
            let entry = entry.map_err(issue)?;
            if entry.file_name().to_string_lossy().starts_with(".tmp-") {
                std::fs::remove_file(entry.path()).map_err(issue)?;
            }
        }
        let Some(bytes) = read_optional(&self.root.join("pending"), RECORD_LIMIT)? else {
            return Ok(());
        };
        let intent: Intent = postcard::from_bytes(&bytes).map_err(issue)?;
        let published = self
            .head()?
            .is_some_and(|head| head.epoch == intent.epoch && head.revision == intent.revision);
        for (kind, key) in intent.keys {
            let directory = self.directory(intent.epoch, &kind, key);
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(issue(error)),
            };
            for entry in entries {
                let entry = entry.map_err(issue)?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let remove = if name.starts_with(".tmp-") {
                    true
                } else if published {
                    name.parse::<u64>()
                        .is_ok_and(|revision| revision != intent.revision)
                } else {
                    name == intent.revision.to_string()
                };
                if remove {
                    std::fs::remove_file(entry.path()).map_err(issue)?;
                }
            }
            let _ = std::fs::remove_dir(directory);
        }
        std::fs::remove_file(self.root.join("pending")).map_err(issue)
    }
    fn directory(&self, epoch: u64, kind: &str, key: (i32, i32)) -> PathBuf {
        self.root
            .join("epochs")
            .join(epoch.to_string())
            .join(kind)
            .join(format!("{},{}", key.0, key.1))
    }
    fn put(
        &self,
        epoch: u64,
        kind: &str,
        key: (i32, i32),
        revision: u64,
        bytes: &[u8],
        limit: usize,
    ) -> Result<(), String> {
        if bytes.len() > limit {
            return Err("spatial record exceeds its internal bound".into());
        }
        let path = self.directory(epoch, kind, key);
        std::fs::create_dir_all(&path).map_err(issue)?;
        let mut framed = paged::hash(bytes).to_vec();
        framed.extend_from_slice(bytes);
        atomic(&path.join(revision.to_string()), &framed)?;
        // Newly created coordinate directories must themselves be durable
        // before the root head can point at their records.
        let mut directory = path.as_path();
        loop {
            std::fs::File::open(directory)
                .and_then(|file| file.sync_all())
                .map_err(issue)?;
            if directory == self.root {
                break;
            }
            directory = directory.parent().ok_or("missing spatial ancestor")?;
        }
        Ok(())
    }
    pub fn page(&self, kind: &str, key: (i32, i32), head: Head) -> Result<Option<Vec<u8>>, String> {
        self.validate_head(head)?;
        let dir = self.directory(head.epoch, kind, key);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(issue(e)),
        };
        let mut latest = None;
        for entry in entries {
            let entry = entry.map_err(issue)?;
            if let Ok(revision) = entry.file_name().to_string_lossy().parse::<u64>()
                && revision <= head.revision
                && latest.is_none_or(|old| revision > old)
            {
                latest = Some(revision);
            }
        }
        let Some(revision) = latest else {
            return Ok(None);
        };
        let limit = if kind.starts_with("canyon") {
            CANYON_RECORD_LIMIT
        } else if kind.starts_with("salvage") {
            SALVAGE_RECORD_LIMIT
        } else {
            RECORD_LIMIT
        };
        let bytes = read_optional(&dir.join(revision.to_string()), limit + 32)?
            .ok_or("missing spatial record")?;
        if bytes.len() < 32 || paged::hash(&bytes[32..]) != bytes[..32] {
            return Err("damaged spatial record".into());
        }
        Ok(Some(bytes[32..].to_vec()))
    }
    fn salvage_index(&self, head: Head) -> Result<Option<SalvageIndex>, String> {
        let Some(raw) = self.page("salvage_index", (0, 0), head)? else {
            return Ok(None);
        };
        if raw.len() > 64 {
            return Err("salvage index exceeds fixed bound".into());
        }
        let index: SalvageIndex = postcard::from_bytes(&raw).map_err(issue)?;
        index.keys().map_err(issue)?;
        Ok(Some(index))
    }
    pub fn salvage(
        &self,
        head: Head,
    ) -> Result<Option<Vec<crate::salvage::SalvageRegionRecord>>, String> {
        let Some(index) = self.salvage_index(head)? else {
            return Ok(None);
        };
        let mut records = Vec::new();
        for key in index.keys().map_err(issue)? {
            let raw = self
                .page("salvage", *key, head)?
                .ok_or("missing salvage region")?;
            records.push(decode_salvage(&raw, *key).map_err(issue)?);
        }
        if !crate::salvage::valid_records(&records) {
            return Err("invalid salvage ownership".into());
        }
        self.validate_head(head)?;
        Ok(Some(records))
    }
    fn canyon_index(&self, head: Head) -> Result<Option<CanyonIndex>, String> {
        let raw = self.page("canyon_index", (0, 0), head)?;
        let raw = match (head.canyon_hash, raw) {
            (None, None) => return Ok(None),
            (Some(expected), Some(raw)) if paged::hash(&raw) == expected => raw,
            _ => return Err("missing or mismatched published Canyon index".into()),
        };
        if raw.len() > CANYON_INDEX_LIMIT {
            return Err("canyon index exceeds fixed bound".into());
        }
        let index: CanyonIndex = postcard::from_bytes(&raw).map_err(issue)?;
        index.keys().map_err(issue)?;
        Ok(Some(index))
    }
    pub fn canyon(
        &self,
        head: Head,
    ) -> Result<Option<Vec<crate::canyon_drift::record::CanyonRegionRecord>>, String> {
        let Some(index) = self.canyon_index(head)? else {
            return Ok(None);
        };
        let mut records = Vec::new();
        for key in index.keys().map_err(issue)? {
            let raw = self
                .page("canyon", *key, head)?
                .ok_or("missing canyon region")?;
            records.push(decode_canyon(&raw, *key).map_err(issue)?);
        }
        if crate::canyon_drift::run::CanyonRun::from_records(&records).is_none() {
            return Err("invalid canyon ownership".into());
        }
        self.validate_head(head)?;
        Ok(Some(records))
    }
    pub fn validate_head(&self, head: Head) -> Result<(), String> {
        if self
            .head()?
            .is_none_or(|h| h.epoch != head.epoch || h.revision != head.revision)
        {
            return Err("spatial read revision expired".into());
        }
        Ok(())
    }
    pub fn active(&self, head: Head) -> Result<Restored, String> {
        let bytes = self
            .page("active", (0, 0), head)?
            .ok_or("missing active spatial record")?;
        if paged::hash(&bytes) != head.active_hash {
            return Err("active spatial record does not match head".into());
        }
        paged::decode(&bytes).map_err(issue)
    }
    /// Full materialization is reserved for portable export and verification.
    /// The runtime paging path reads `active` plus nearby coordinate records.
    pub fn materialize(&self, head: Head) -> Result<Restored, String> {
        let mut restored = self.active(head)?;
        for key in self.keys(head.epoch, "terrain")? {
            if let Some(bytes) = self.page("terrain", key, head)? {
                let cut = SleepingCut::decode(&bytes).map_err(issue)?;
                if cut.key != key {
                    return Err("spatial terrain coordinate mismatch".into());
                }
                restored.terrain.restore_sleeping_cut(cut)?;
            }
        }
        if let Some(memory) = &mut restored.player.memory {
            for key in self.keys(head.epoch, "memory")? {
                if let Some(bytes) = self.page("memory", key, head)? {
                    let page: MemoryChunk = postcard::from_bytes(&bytes).map_err(issue)?;
                    if (page.x as i32, page.y as i32) != key {
                        return Err("spatial memory coordinate mismatch".into());
                    }
                    memory.cells.push(page);
                }
            }
            memory.complete = true;
        }
        let small = restored.terrain.resident_sleeping_chunks() <= 64;
        let clean = restored.terrain.dirty_sleeping();
        restored.terrain.acknowledge_sleeping(&clean);
        let pages = restored
            .player
            .memory
            .as_mut()
            .map(|memory| std::mem::take(&mut memory.cells));
        let mut restored = paged::restored(
            restored.terrain,
            &serde_json::to_vec(&restored.player).map_err(issue)?,
            restored.sim_tick,
            small,
        )
        .map_err(issue)?;
        if let (Some(memory), Some(pages)) = (&mut restored.player.memory, pages) {
            memory.cells = pages;
        }
        restored.salvage = self.salvage(head)?;
        restored.canyon = self.canyon(head)?;
        self.validate_head(head)?;
        Ok(restored)
    }
    /// Startup probes a bounded number of coordinate keys. Small saves retain
    /// complete replay starts; large saves install only the active kernel and
    /// leave distant terrain and personal pages to asynchronous reads.
    pub fn startup(&self, head: Head) -> Result<Restored, String> {
        let mut restored = self.active(head)?;
        let terrain_keys = self.keys_limited(head.epoch, "terrain", 65)?;
        let memory_keys = self.keys_limited(
            head.epoch,
            "memory",
            crate::page_cache::RESIDENT_PAGE_LIMIT + 1,
        )?;
        let small =
            terrain_keys.len() <= 64 && memory_keys.len() <= crate::page_cache::RESIDENT_PAGE_LIMIT;
        if terrain_keys.len() <= 64 {
            for key in terrain_keys {
                if let Some(bytes) = self.page("terrain", key, head)? {
                    let cut = SleepingCut::decode(&bytes).map_err(issue)?;
                    if cut.key != key {
                        return Err("spatial terrain coordinate mismatch".into());
                    }
                    restored.terrain.restore_sleeping_cut(cut)?;
                }
            }
        }
        let clean = restored.terrain.dirty_sleeping();
        restored.terrain.acknowledge_sleeping(&clean);
        let thin = serde_json::to_vec(&restored.player).map_err(issue)?;
        restored =
            paged::restored(restored.terrain, &thin, restored.sim_tick, small).map_err(issue)?;
        if memory_keys.len() <= crate::page_cache::RESIDENT_PAGE_LIMIT
            && let Some(memory) = &mut restored.player.memory
        {
            for key in memory_keys {
                if let Some(bytes) = self.page("memory", key, head)? {
                    let page: MemoryChunk = postcard::from_bytes(&bytes).map_err(issue)?;
                    if (page.x as i32, page.y as i32) != key {
                        return Err("spatial memory coordinate mismatch".into());
                    }
                    memory.cells.push(page);
                }
            }
            memory.complete = true;
        }
        restored.terrain.enable_paging();
        restored.salvage = self.salvage(head)?;
        restored.canyon = self.canyon(head)?;
        self.validate_head(head)?;
        Ok(restored)
    }
    fn keys(&self, epoch: u64, kind: &str) -> Result<Vec<(i32, i32)>, String> {
        self.keys_limited(epoch, kind, usize::MAX)
    }
    fn keys_limited(
        &self,
        epoch: u64,
        kind: &str,
        limit: usize,
    ) -> Result<Vec<(i32, i32)>, String> {
        let entries =
            match std::fs::read_dir(self.root.join("epochs").join(epoch.to_string()).join(kind)) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(issue(e)),
            };
        entries
            .take(limit)
            .map(|entry| {
                let entry = entry.map_err(issue)?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let (x, y) = name.split_once(',').ok_or("invalid spatial key")?;
                Ok((x.parse().map_err(issue)?, y.parse().map_err(issue)?))
            })
            .collect()
    }
}
fn read_optional(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(issue(e)),
    };
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(issue)?;
    if bytes.len() > limit {
        return Err("spatial record exceeds its internal bound".into());
    }
    Ok(Some(bytes))
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("missing spatial parent")?;
    std::fs::create_dir_all(parent).map_err(issue)?;
    let temp = parent.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(issue)?;
    let result = (|| {
        file.write_all(bytes).map_err(issue)?;
        file.sync_all().map_err(issue)?;
        drop(file);
        std::fs::rename(&temp, path).map_err(issue)?;
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(issue)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
fn issue(error: impl std::fmt::Debug) -> String {
    format!("spatial save: {error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Memory, SAVE_VERSION, SaveData, Tuning, underwater::World};
    fn fixture(name: &str) -> (Store, World, Memory) {
        let root = std::env::temp_dir().join(format!("crab-spatial-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let level = Level::test_level();
        let memory = Memory::new(&level, &Tuning::default().memory);
        (Store::new(root), World::from_level_settled(&level), memory)
    }
    fn capture(world: &World, memory: &Memory, tick: u64) -> Capture {
        let (left, bottom, width, height) = world.game_bounds();
        Capture::new(
            SaveData {
                version: SAVE_VERSION,
                x: left + width * 0.5,
                y: bottom + height * 0.5,
                memory: Some(memory.metadata()),
                progress: None,
            },
            world,
            memory,
            tick,
        )
        .unwrap()
    }
    fn canyon_at(generation: u32) -> crate::canyon_drift::run::CanyonRun {
        use crate::canyon_drift::run::{CanyonRun, SlotPhase};
        use bevy_math::Vec2;
        let mut run = CanyonRun::new(71);
        for (i, slot) in run.slots.iter_mut().enumerate() {
            slot.generation = generation;
            slot.revision = u64::from(generation) * 3;
            slot.location = Vec2::new(
                10_000.0 + generation as f32 * 512.0 + i as f32 * 1024.0,
                40.0,
            );
            slot.phase = SlotPhase::WashedOut;
        }
        run.catcher.position = Vec2::new(30_000.0 + generation as f32 * 512.0, 40.0);
        run.catcher.revision = u64::from(generation);
        run.practice.position = Vec2::new(50_000.0 + generation as f32 * 512.0, 40.0);
        run.practice_revision = u64::from(generation);
        assert!(run.complete());
        run
    }

    #[test]
    fn canyon_relocation_reclaims_stale_keys_and_acknowledges_only_the_capture() {
        let (store, world, memory) = fixture("canyon-moves");
        let mut previous = None;
        for generation in 1..=12 {
            let mut live = canyon_at(generation);
            let records = live.records();
            assert_eq!(records.len(), 5);
            let candidate = capture(&world, &memory, 120 + u64::from(generation))
                .with_canyon(records.clone())
                .unwrap();
            let expected_player: SaveData =
                serde_json::from_slice(&candidate.active.player).unwrap();
            live.catcher.revision += 1;
            let ack = store.commit(candidate, 0).unwrap();
            assert_eq!(ack.player, Some(expected_player));
            assert_eq!(ack.canyon, Some(records.clone()));
            assert_ne!(
                ack.canyon,
                Some(live.records()),
                "RAM after capture is not durable evidence"
            );
            let head = store.head().unwrap().unwrap();
            assert_eq!(store.startup(head).unwrap().canyon, Some(records.clone()));
            assert_eq!(store.materialize(head).unwrap().canyon, Some(records));
            assert_eq!(store.keys(0, "canyon").unwrap().len(), 5);
            assert_eq!(store.keys(0, "canyon_index").unwrap().len(), 1);
            if let Some(old) = previous {
                assert!(store.canyon(old).is_err());
            }
            previous = Some(head);
        }
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn failed_canyon_move_keeps_player_terrain_and_previous_records_atomic() {
        use std::cell::Cell;
        let (store, world, memory) = fixture("canyon-abort");
        let original = canyon_at(1).records();
        store
            .commit(
                capture(&world, &memory, 120)
                    .with_canyon(original.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        let before = store.head().unwrap().unwrap();
        let old = store.startup(before).unwrap();
        let mut candidate = capture(&world, &memory, 121)
            .with_canyon(canyon_at(2).records())
            .unwrap();
        let mut player: SaveData = serde_json::from_slice(&candidate.active.player).unwrap();
        player.x += 5.0;
        candidate.active.player = serde_json::to_vec(&player).unwrap();
        let checks = Cell::new(0);
        assert!(
            store
                .commit_checked(candidate, 0, || {
                    let n = checks.get();
                    checks.set(n + 1);
                    n == 0
                })
                .is_err()
        );
        assert_eq!(store.head().unwrap().unwrap().revision, before.revision);
        let after = store.startup(before).unwrap();
        assert_eq!(after.player, old.player);
        assert_eq!(after.terrain.checkpoint(), old.terrain.checkpoint());
        assert_eq!(after.canyon, Some(original.clone()));
        assert!(Acknowledgement::default().canyon.is_none());
        store
            .commit(
                capture(&world, &memory, 122)
                    .with_canyon(original.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(
            store.canyon(store.head().unwrap().unwrap()).unwrap(),
            Some(original)
        );
        assert_eq!(store.keys(0, "canyon").unwrap().len(), 5);
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn canyon_reset_rejects_old_epoch_and_drops_all_old_coordinate_owners() {
        let (store, world, memory) = fixture("canyon-reset");
        store
            .commit(
                capture(&world, &memory, 120)
                    .with_canyon(canyon_at(1).records())
                    .unwrap(),
                0,
            )
            .unwrap();
        let stale = capture(&world, &memory, 121)
            .with_canyon(canyon_at(2).records())
            .unwrap();
        store.reset().unwrap();
        let reset = store.head().unwrap().unwrap();
        assert_eq!(reset.revision, 0);
        assert!(store.canyon(reset).unwrap().is_none());
        assert!(store.commit(stale, 0).is_err());
        assert!(store.keys(0, "canyon").unwrap().is_empty());
        let fresh = canyon_at(3).records();
        let ack = store
            .commit(
                capture(&world, &memory, 122)
                    .with_canyon(fresh.clone())
                    .unwrap(),
                reset.epoch,
            )
            .unwrap();
        assert_eq!(ack.canyon, Some(fresh.clone()));
        assert_eq!(
            store
                .startup(store.head().unwrap().unwrap())
                .unwrap()
                .canyon,
            Some(fresh)
        );
        assert_eq!(store.keys(reset.epoch, "canyon").unwrap().len(), 5);
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn published_canyon_index_cannot_disappear_or_be_reset_by_a_partial_capture() {
        let (store, world, memory) = fixture("canyon-index-presence");
        store.commit(capture(&world, &memory, 120), 0).unwrap();
        let never_created = store.head().unwrap().unwrap();
        assert!(never_created.canyon_hash.is_none());
        assert!(store.startup(never_created).unwrap().canyon.is_none());
        let records = canyon_at(1).records();
        store
            .commit(
                capture(&world, &memory, 121)
                    .with_canyon(records.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        let published = store.head().unwrap().unwrap();
        assert!(published.canyon_hash.is_some());
        // A terrain-only capture preserves the marker and old authored state.
        let ack = store.commit(capture(&world, &memory, 122), 0).unwrap();
        assert!(ack.canyon.is_none());
        let current = store.head().unwrap().unwrap();
        assert_eq!(current.canyon_hash, published.canyon_hash);
        assert_eq!(store.startup(current).unwrap().canyon, Some(records));
        std::fs::remove_dir_all(store.directory(0, "canyon_index", (0, 0))).unwrap();
        assert!(store.canyon(current).is_err());
        assert!(
            store.startup(current).is_err(),
            "a missing published anchor index must never create a fresh Canyon"
        );
        assert_eq!(store.head().unwrap().unwrap().revision, current.revision);
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn canyon_player_receipt_and_terminal_record_commit_together_or_not_at_all() {
        use crate::canyon_drift::{
            DriftProfile,
            run::{ANCHOR, CanyonRun, DriftClaimId, Grant, GrantPhase, SlotPhase},
        };
        use std::cell::Cell;
        let (store, world, memory) = fixture("canyon-player-terminal");
        let id = DriftClaimId {
            world: 71,
            anchor: ANCHOR,
            slot: 0,
            generation: 7,
        };
        let mut run = CanyonRun::new(71);
        run.slots[0].generation = 7;
        run.slots[0].revision = 100;
        let grant = Grant {
            id,
            player: crate::PlayerId(0),
            identity: 99,
            profile: DriftProfile::Flat,
            phase: GrantPhase::FinalizedSaved,
        };
        run.slots[0].phase = SlotPhase::Grant(grant);
        let make = |tick, receipts: Vec<DriftClaimId>, run: &CanyonRun| {
            let mut candidate = capture(&world, &memory, tick)
                .with_canyon(run.records())
                .unwrap();
            let mut player: SaveData = serde_json::from_slice(&candidate.active.player).unwrap();
            player.progress = Some(crate::Progress {
                salvage_identity: 99,
                drift_receipts: receipts,
                ..crate::Sim::new(Tuning::default(), Level::test_level()).fresh_progress()
            });
            candidate.active.player = serde_json::to_vec(&player).unwrap();
            candidate
        };
        let old_ack = store.commit(make(120, vec![id], &run), 0).unwrap();
        let old_head = store.head().unwrap().unwrap();
        run.slots[0].phase = SlotPhase::Grant(Grant {
            phase: GrantPhase::ReceiptRetired,
            ..grant
        });
        run.slots[0].revision = 101;
        let checks = Cell::new(0);
        assert!(
            store
                .commit_checked(make(121, vec![], &run), 0, || {
                    let n = checks.get();
                    checks.set(n + 1);
                    n == 0
                })
                .is_err()
        );
        let previous = store.startup(old_head).unwrap();
        assert_eq!(previous.player.progress.unwrap().drift_receipts, vec![id]);
        let durable = CanyonRun::from_records(previous.canyon.as_ref().unwrap()).unwrap();
        assert!(matches!(
            durable.slots[0].phase,
            SlotPhase::Grant(Grant {
                phase: GrantPhase::FinalizedSaved,
                ..
            })
        ));
        let old_revision = CanyonRun::from_records(old_ack.canyon.as_ref().unwrap())
            .unwrap()
            .slots[0]
            .revision;
        assert!(!run.retire_durable(id, old_revision));
        let ack = store.commit(make(122, vec![], &run), 0).unwrap();
        assert!(
            ack.player
                .as_ref()
                .unwrap()
                .progress
                .as_ref()
                .unwrap()
                .drift_receipts
                .is_empty()
        );
        let saved = CanyonRun::from_records(ack.canyon.as_ref().unwrap()).unwrap();
        assert_eq!(saved.slots[0].revision, 101);
        assert!(run.retire_durable(id, saved.slots[0].revision));
        let restored = store.startup(store.head().unwrap().unwrap()).unwrap();
        assert!(restored.player.progress.unwrap().drift_receipts.is_empty());
        assert_eq!(restored.canyon, ack.canyon);
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn canyon_terminal_ack_cannot_retire_a_newer_ram_revision() {
        use crate::canyon_drift::{
            DriftProfile,
            run::{ANCHOR, CanyonRun, DriftClaimId, Grant, GrantPhase, SlotPhase},
        };
        let (store, world, memory) = fixture("canyon-terminal-ack");
        let id = DriftClaimId {
            world: 71,
            anchor: ANCHOR,
            slot: 0,
            generation: 1,
        };
        let mut run = CanyonRun::new(71);
        run.slots[0].generation = 1;
        run.slots[0].revision = 5;
        let grant = Grant {
            id,
            player: crate::PlayerId(0),
            identity: 99,
            profile: DriftProfile::Flat,
            phase: GrantPhase::FinalizedSaved,
        };
        run.slots[0].phase = SlotPhase::Grant(grant);
        let ack = store
            .commit(
                capture(&world, &memory, 120)
                    .with_canyon(run.records())
                    .unwrap(),
                0,
            )
            .unwrap();
        let committed = CanyonRun::from_records(ack.canyon.as_ref().unwrap()).unwrap();
        assert!(
            !run.retire_durable(id, committed.slots[0].revision),
            "terminal/receipt-retired state has not committed"
        );
        run.slots[0].phase = SlotPhase::Grant(Grant {
            phase: GrantPhase::ReceiptRetired,
            ..grant
        });
        run.slots[0].revision += 1;
        assert!(
            !run.retire_durable(id, committed.slots[0].revision),
            "old ACK is not evidence for new receipt state"
        );
        let ack = store
            .commit(
                capture(&world, &memory, 121)
                    .with_canyon(run.records())
                    .unwrap(),
                0,
            )
            .unwrap();
        let committed = CanyonRun::from_records(ack.canyon.as_ref().unwrap()).unwrap();
        assert!(run.retire_durable(id, committed.slots[0].revision));
        assert_eq!(run.slots[0].phase, SlotPhase::Vacant);
        assert!(matches!(
            CanyonRun::from_records(
                &store
                    .canyon(store.head().unwrap().unwrap())
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
            .slots[0]
                .phase,
            SlotPhase::Grant(Grant {
                phase: GrantPhase::ReceiptRetired,
                ..
            })
        ));
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn retired_native_envelope_starts_a_new_epoch_without_loading_old_records() {
        let (store, world, memory) = fixture("canyon-retired-envelope");
        store
            .commit(
                capture(&world, &memory, 120)
                    .with_canyon(canyon_at(1).records())
                    .unwrap(),
                0,
            )
            .unwrap();
        let old = store.head().unwrap().unwrap();
        let mut bytes = b"CRABSP01".to_vec();
        bytes.extend(postcard::to_allocvec(&(old.epoch, old.revision, old.active_hash)).unwrap());
        std::fs::write(store.root.join("head"), bytes).unwrap();
        assert!(store.head().is_err());
        assert!(store.retire_obsolete().unwrap());
        let fresh = store.head().unwrap().unwrap();
        assert_eq!((fresh.epoch, fresh.revision), (1, 0));
        assert!(store.keys(0, "canyon").unwrap().is_empty());
        assert!(!store.retire_obsolete().unwrap());
        store
            .commit(
                capture(&world, &memory, 121)
                    .with_canyon(canyon_at(2).records())
                    .unwrap(),
                fresh.epoch,
            )
            .unwrap();
        assert!(
            store
                .startup(store.head().unwrap().unwrap())
                .unwrap()
                .canyon
                .is_some()
        );
        std::fs::remove_dir_all(store.root).unwrap();
    }

    #[test]
    fn salvage_relocation_is_bounded_and_preserves_physical_state_on_restart() {
        use crate::salvage::{FindPhase, Shellwash};
        let (store, world, memory) = fixture("salvage-moves");
        let mut site = Shellwash {
            phase: FindPhase::Free,
            ..Shellwash::default()
        };
        for step in 0..16 {
            site.position.x = 300.0 + step as f32 * 300.0;
            site.velocity.x = step as f32;
            site.fragment_positions = [site.position; 2];
            let records = site.records();
            let prepared = capture(&world, &memory, 120 + step)
                .with_salvage(records.clone())
                .unwrap();
            let expected_player: crate::SaveData =
                serde_json::from_slice(&prepared.active.player).unwrap();
            let ack = store.commit(prepared, 0).unwrap();
            assert_eq!(ack.player, Some(expected_player));
            assert_eq!(ack.salvage, Some(records.clone()));
            let head = store.head().unwrap().unwrap();
            let restored = store.startup(head).unwrap();
            assert_eq!(restored.salvage, Some(records));
            assert!(
                store.keys(0, "salvage").unwrap().len() <= 4,
                "old spatial owners must be reclaimed"
            );
            assert_eq!(store.keys(0, "salvage_index").unwrap().len(), 1);
        }
        site.phase = FindPhase::Consumed;
        site.fragments = [true, true];
        site.fragment_positions = [
            bevy_math::Vec2::new(8000.0, 10.0),
            bevy_math::Vec2::new(9000.0, 10.0),
        ];
        let records = site.records();
        assert_eq!(records.len(), 4);
        store
            .commit(
                capture(&world, &memory, 150)
                    .with_salvage(records.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(
            store.salvage(store.head().unwrap().unwrap()).unwrap(),
            Some(records)
        );
        site.fragment_positions[0].x += 512.0;
        site.fragment_positions[1].x += 512.0;
        let relocated = site.records();
        store
            .commit(
                capture(&world, &memory, 151)
                    .with_salvage(relocated.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(
            store.salvage(store.head().unwrap().unwrap()).unwrap(),
            Some(relocated)
        );
        assert!(store.keys(0, "salvage").unwrap().len() <= 4);
        store.reset().unwrap();
        assert!(
            store
                .salvage(store.head().unwrap().unwrap())
                .unwrap()
                .is_none()
        );
        std::fs::remove_dir_all(store.root).unwrap();
    }
    #[test]
    fn failed_salvage_move_keeps_previous_world_and_retry_reclaims_orphans() {
        use crate::salvage::Shellwash;
        use std::cell::Cell;
        let (store, world, memory) = fixture("salvage-abort");
        let mut site = Shellwash::default();
        let original = site.records();
        store
            .commit(
                capture(&world, &memory, 120)
                    .with_salvage(original.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        let before = store.head().unwrap().unwrap();
        let before_player = store.active(before).unwrap().player;
        site.position.x = 5000.0;
        site.fragment_positions = [site.position; 2];
        let mut candidate = capture(&world, &memory, 121)
            .with_salvage(site.records())
            .unwrap();
        let mut next_player: crate::SaveData =
            serde_json::from_slice(&candidate.active.player).unwrap();
        next_player.x += 5.0;
        candidate.active.player = serde_json::to_vec(&next_player).unwrap();
        let current = Cell::new(0);
        assert!(
            store
                .commit_checked(candidate, 0, || {
                    let n = current.get();
                    current.set(n + 1);
                    n == 0
                })
                .is_err()
        );
        assert_eq!(store.head().unwrap().unwrap().revision, before.revision);
        assert_eq!(store.salvage(before).unwrap(), Some(original.clone()));
        assert_eq!(store.active(before).unwrap().player, before_player);
        assert!(Acknowledgement::default().player.is_none());
        assert!(Acknowledgement::default().salvage.is_none());
        store
            .commit(
                capture(&world, &memory, 122)
                    .with_salvage(original.clone())
                    .unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(
            store.salvage(store.head().unwrap().unwrap()).unwrap(),
            Some(original)
        );
        assert!(store.keys(0, "salvage").unwrap().len() <= 4);
        std::fs::remove_dir_all(store.root).unwrap();
    }
    #[test]
    fn changed_records_round_trip_and_old_readers_are_revoked() {
        let (store, mut world, mut memory) = fixture("roundtrip");
        store
            .commit(capture(&world, &memory, 120), 0)
            .unwrap()
            .apply(&mut world, &mut memory);
        let old = store.head().unwrap().unwrap();
        assert!(world.follow_player(&Level::test_level(), 4400.0, -700.0));
        let second = world.checkpoint();
        let frozen = capture(&world, &memory, 240);
        assert!(
            frozen.memory.is_empty(),
            "unchanged memory is not recaptured"
        );
        store
            .commit(frozen, 0)
            .unwrap()
            .apply(&mut world, &mut memory);
        let new = store.head().unwrap().unwrap();
        assert!(store.materialize(old).is_err());
        assert_eq!(store.materialize(new).unwrap().terrain.checkpoint(), second);
        assert!(std::fs::metadata(store.root.join("head")).unwrap().len() <= 128);
        assert!(capture(&world, &memory, 241).sleeping.is_empty());
        std::fs::remove_dir_all(store.root).unwrap();
    }
    #[test]
    fn reset_rejects_stale_writers_and_changed_coordinates_reclaim_history() {
        let (store, mut world, mut memory) = fixture("reset-gc");
        store
            .commit(capture(&world, &memory, 120), 0)
            .unwrap()
            .apply(&mut world, &mut memory);
        let old = store.head().unwrap().unwrap();
        for tick in 121..125 {
            store.commit(capture(&world, &memory, tick), 0).unwrap();
        }
        assert_eq!(
            std::fs::read_dir(store.directory(0, "active", (0, 0)))
                .unwrap()
                .count(),
            1
        );
        store.reset().unwrap();
        let reset = store.head().unwrap().unwrap();
        assert_eq!((reset.epoch, reset.revision), (1, 0));
        assert!(
            store
                .commit_at(
                    capture(&world, &memory, 130),
                    0,
                    Some((old.epoch, old.revision)),
                    || true
                )
                .is_err()
        );
        store
            .commit_at(capture(&world, &memory, 131), 1, Some((1, 0)), || true)
            .unwrap();
        let saved = store.head().unwrap().unwrap();
        assert!(store.page("terrain", (100, 100), saved).unwrap().is_none());
        std::fs::remove_dir_all(store.root).unwrap();
    }
    #[test]
    fn canceled_commit_keeps_previous_head_and_cannot_leak_orphan_pages_into_retry() {
        let (store, mut world, mut memory) = fixture("interrupted");
        store
            .commit(capture(&world, &memory, 120), 3)
            .unwrap()
            .apply(&mut world, &mut memory);
        let old = store.head().unwrap().unwrap();
        let original = store.materialize(old).unwrap();
        assert!(world.follow_player(&Level::test_level(), 4400.0, -700.0));
        let calls = std::cell::Cell::new(0);
        assert!(
            store
                .commit_checked(capture(&world, &memory, 240), 3, || {
                    let n = calls.get();
                    calls.set(n + 1);
                    n == 0
                })
                .is_err()
        );
        assert_eq!(store.head().unwrap().unwrap().revision, old.revision);
        assert_eq!(
            store.materialize(old).unwrap().terrain.checkpoint(),
            original.terrain.checkpoint()
        );
        // Model a process restart from the last complete head, so failed
        // generation keys are absent from this new world's dirty set.
        let restored_memory = Memory::from_save(
            original.player.memory.as_ref().unwrap(),
            &Level::test_level(),
            &Tuning::default().memory,
        )
        .unwrap();
        store
            .commit(capture(&original.terrain, &restored_memory, 121), 3)
            .unwrap();
        let retried = store.materialize(store.head().unwrap().unwrap()).unwrap();
        assert_eq!(retried.terrain.checkpoint(), original.terrain.checkpoint());
        assert!(
            store
                .commit(capture(&original.terrain, &restored_memory, 122), 2)
                .is_err()
        );
        std::fs::remove_dir_all(store.root).unwrap();
    }
}
