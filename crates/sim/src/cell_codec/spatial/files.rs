//! Native spatial records. A small head advances only after all changed
//! records have reached disk. Unpublished generations are invisible to reads.
use super::{Acknowledgement, Capture};
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

const MAGIC: &[u8; 8] = b"CRABSP01";
const RECORD_LIMIT: usize = 8 * 1024 * 1024;
const PAGE_LIMIT: usize = 256 * 1024;
static TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Head {
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
    pub fn reset(&self) -> Result<(), String> {
        let _lock = self.lock()?;
        let epoch = self
            .head()?
            .map_or(Ok(1), |h| {
                h.epoch.checked_add(1).ok_or("save epoch exhausted")
            })
            .map_err(str::to_owned)?;
        let head = Head {
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
        let head = Head {
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
        let _ = std::fs::remove_file(self.root.join("pending"));
        Ok(Acknowledgement {
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
            let entries = match std::fs::read_dir(directory) {
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
        let bytes = read_optional(&dir.join(revision.to_string()), RECORD_LIMIT + 32)?
            .ok_or("missing spatial record")?;
        if bytes.len() < 32 || paged::hash(&bytes[32..]) != bytes[..32] {
            return Err("damaged spatial record".into());
        }
        Ok(Some(bytes[32..].to_vec()))
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
