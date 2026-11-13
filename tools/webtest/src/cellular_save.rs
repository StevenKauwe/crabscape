//! Full game regression, using the same WebKit session and phone frame as the other checks.
use crabsim::{
    Level, Replay, Sim,
    underwater::{GAME_CELL, HEIGHT, WIDTH, World},
};
use serde_json::Value;

use crate::{CANVAS, Test, driver::Result, settings_button, settings_tab, start_over_centre};

#[path = "fixture_install.rs"]
mod fixture_install;

const DATABASE: &str = "crabscape.cells";
const MARKER: (f32, f32) = (1000.0, 700.0);

struct Saved {
    tick: u64,
    player: Value,
    terrain: World,
}

/// A stable rock cell absent from the normal level catches player-only restoration.
fn fixture() -> Result<Vec<u8>> {
    let mut level = Level::test_level();
    let mut rock = level.home;
    rock.min.x = MARKER.0 - 8.0;
    rock.max.x = MARKER.0 + 8.0;
    rock.min.y = MARKER.1 - 8.0;
    rock.max.y = MARKER.1 + 8.0;
    level.rocks.push(rock);
    let terrain = World::from_level_settled(&level);
    if !terrain.game_rock_at(MARKER.0, MARKER.1)
        || World::from_level_settled(&Level::test_level()).game_rock_at(MARKER.0, MARKER.1)
    {
        return Err("cellular fixture marker does not distinguish saved and fresh terrain".into());
    }
    let player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: 400.0,
        y: 0.0,
        memory: None,
        progress: None,
    };
    crabsim::cell_codec::paged::prepare(&player, &terrain, 17)
        .and_then(|prepared| prepared.encode())
        .map_err(|e| format!("fixture encode: {e:?}"))
}

/// Simulate exploration far beyond the original browser checkpoint limit.
fn large_fixture() -> Result<Vec<u8>> {
    let level = Level::test_level();
    let mut terrain = World::from_level_settled(&level);
    for (x, y) in [
        (4400.0, -700.0),
        (8000.0, -1400.0),
        (4000.0, -3000.0),
        (8000.0, -5000.0),
        (3000.0, -7000.0),
        (8000.0, -9000.0),
        (400.0, 0.0),
    ] {
        if !terrain.follow_player(&level, x, y) {
            return Err("fixture did not stream".into());
        }
        for _ in 0..4 {
            terrain.step(0.35);
        }
    }
    let snapshot = crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&terrain)
        .map_err(|e| format!("large snapshot: {e:?}"))?;
    let chunks = snapshot.sleeping_ids().count();
    let raw = postcard::to_allocvec(&terrain.checkpoint()).map_err(|e| e.to_string())?;
    if chunks <= 256 || raw.len() <= 12 * 1024 * 1024 {
        return Err(format!(
            "fixture is too small: {chunks} sleeping chunks, {} raw bytes",
            raw.len()
        ));
    }
    drop(raw);
    let player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: 400.0,
        y: 0.0,
        memory: None,
        progress: None,
    };
    crabsim::cell_codec::paged::prepare(&player, &terrain, 17)
        .and_then(|p| p.encode())
        .map_err(|e| format!("large fixture: {e:?}"))
}

fn decode(bytes: &[u8]) -> Result<Saved> {
    let value = crabsim::cell_codec::paged::decode(bytes)
        .map_err(|e| format!("invalid paged save: {e:?}"))?;
    Ok(Saved {
        tick: value.sim_tick,
        player: serde_json::to_value(&value.player).map_err(|e| e.to_string())?,
        terrain: value.terrain,
    })
}

// Both browser drivers install and capture the actual spatial format.
#[path = "../../cooptest/src/save_fixture.rs"]
#[allow(dead_code)]
mod spatial_fixture;
use spatial_fixture::{frame_from_object, spatial_object_from_frame as object_from_frame};

fn terrain_has_marker(world: &World) -> bool {
    if world.contains_world(MARKER.0, MARKER.1) {
        return world.game_rock_at(MARKER.0, MARKER.1);
    }
    // Digging can move the active window below the marker. Reactivate its
    // saved sleeping chunk on a copy, keeping the exact restoration check
    // untouched. Use the normal level: regenerating a missing chunk must
    // not recreate the fixture's extra rock and accidentally pass the test.
    let mut inspected = world.clone();
    inspected.follow_player(&Level::test_level(), MARKER.0, MARKER.1);
    inspected.game_rock_at(MARKER.0, MARKER.1)
}

/// Compare actual materials in overlapping world coordinates. Tick, current,
/// particle and other animation-only changes cannot satisfy this assertion.
fn changed_material_cells(before: &World, after: &World) -> usize {
    let (left, bottom, _, _) = before.game_bounds();
    let mut changed = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let wx = left + (x as f32 + 0.5) * GAME_CELL;
            let wy = bottom + (y as f32 + 0.5) * GAME_CELL;
            if after.contains_world(wx, wy) {
                let (ax, ay) = after.local_cell(wx, wy);
                changed += usize::from(
                    before.cell(x as i32, y as i32) != after.cell(ax as i32, ay as i32),
                );
            }
        }
    }
    changed
}

// Lightweight test-only inspection: one read-only transaction returns the
// committed head and the player record it names. Rust decodes both with the
// production codecs, so a head layout change cannot silently turn the tick
// into garbage (an ad-hoc JS varint parse did, once `canyon_hash` was added
// in front of `tick`, and let the large-save wait pass before any new tick).
const READ_PLAYER: &str = r#"await new Promise((resolve,reject)=>{
 const opening=indexedDB.open('crabscape.cells',1);opening.onerror=()=>reject(String(opening.error));
 opening.onsuccess=()=>{
  const db=opening.result,tx=db.transaction('world','readonly'),store=tx.objectStore('world');
  const head=store.get('active'),epoch=store.get('reset_epoch');let player;
  epoch.onsuccess=()=>{const bytes=epoch.result;const n=bytes?new DataView(bytes.buffer,bytes.byteOffset,8).getBigUint64(0,true):0n;
   player=store.get([n.toString().padStart(20,'0'),'player',0,0,'00000000000000000000']);};
  tx.onabort=()=>{db.close();reject(String(tx.error));};
  tx.oncomplete=()=>{db.close();
   const raw=head.result,data=player&&player.result;
   if(!(raw instanceof Uint8Array)||!(data instanceof Uint8Array)){resolve(null);return;}
   const encode=bytes=>{const pieces=[];for(let i=0;i<bytes.length;i+=32768)pieces.push(String.fromCharCode(...bytes.subarray(i,i+32768)));return btoa(pieces.join(''));};
   resolve({head:encode(raw),player:encode(data)});};
 };
})"#;

/// A committed spatial save as the game published it: its head and the
/// player record authenticated against that head's revision.
struct Committed {
    head: crabsim::cell_codec::spatial::SpatialHead,
    player: Value,
}

fn committed_from(value: &Value) -> Result<Option<Committed>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use crabsim::cell_codec::spatial::{SPATIAL_MAGIC, SpatialHead, decode_record};
    if value.is_null() {
        return Ok(None);
    }
    let bytes = |field: &str| -> Result<Vec<u8>> {
        STANDARD
            .decode(value[field].as_str().ok_or("missing committed field")?)
            .map_err(|e| e.to_string())
    };
    let raw = bytes("head")?;
    if !raw.starts_with(SPATIAL_MAGIC) {
        return Ok(None);
    }
    let head: SpatialHead =
        postcard::from_bytes(&raw[SPATIAL_MAGIC.len()..]).map_err(|e| format!("head: {e}"))?;
    let player = decode_record(&bytes("player")?, head.revision, 4 * 1024 * 1024)
        .map_err(|e| format!("player record does not belong to its head: {e:?}"))?;
    let player = serde_json::from_slice(&player).map_err(|e| e.to_string())?;
    Ok(Some(Committed { head, player }))
}

fn preserved_normal_save() -> crabsim::SaveData {
    crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: 918.0,
        y: Level::test_level().floor_height(918.0),
        memory: None,
        progress: None,
    }
}

impl Test<'_> {
    fn close_cellular_replay(&self) -> Result<()> {
        let focused = self.eval("await (() => { document.getElementById('replay-close').click(); return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(document.activeElement === document.getElementById('crabscape') && document.getElementById('replay-box').hidden)))); })()")?;
        if focused != serde_json::Value::Bool(true) {
            return Err("closing the cellular replay did not restore game focus".into());
        }
        Ok(())
    }

    // The confirmation expires after four seconds. Do not export and rebuild
    // the entire world between taps just to ask whether its checkpoint exists.
    fn cellular_record_exists(&self) -> Result<bool> {
        let exists = self.eval(
            r#"await new Promise((resolve, reject) => {
            const opening = indexedDB.open('crabscape.cells', 1);
            opening.onerror = () => reject(String(opening.error));
            opening.onsuccess = () => {
                const db = opening.result, tx = db.transaction('world', 'readonly');
                const key = tx.objectStore('world').getKey('active');
                tx.oncomplete = () => { db.close(); resolve(key.result !== undefined); };
                tx.onabort = () => { db.close(); reject(String(tx.error)); };
            };
        })"#,
        )?;
        Ok(exists == Value::Bool(true))
    }

    fn committed(&self) -> Result<Option<Committed>> {
        committed_from(&self.eval(READ_PLAYER)?)
    }

    /// Poll the committed save (not a timer) until `done` accepts it.
    fn wait_for_commit(&self, seconds: f64, done: impl Fn(&Committed) -> bool) -> Result<bool> {
        let end = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
        while std::time::Instant::now() < end {
            if self.committed()?.as_ref().is_some_and(&done) {
                return Ok(true);
            }
            crate::pump(0.25);
        }
        Ok(false)
    }

    fn cellular_record(&self) -> Result<Option<Vec<u8>>> {
        let record = self.eval(&format!("await ({})", spatial_fixture::CAPTURE_SCRIPT))?;
        if record.is_null() {
            Ok(None)
        } else {
            frame_from_object(&record).map(Some)
        }
    }

    fn cellular_replay(&self) -> Result<Replay> {
        self.eval(&format!("({CANVAS}.removeAttribute('data-replay'), true)"))?;
        self.press("F9", "F9")?;
        if !self.wait_for(&format!("{CANVAS}.hasAttribute('data-replay')"), 15.0)? {
            return Err("the resumed cellular game did not export its replay".into());
        }
        Replay::parse(&self.attr("data-replay")?.ok_or("missing cellular replay")?)
    }

    pub(crate) fn cellular_autosave(&mut self) -> Result<()> {
        // Stop the previous game before replacing its fixture; an old autosave must not race setup.
        self.go("assets/crab/crab-atlas.png")?;
        let bytes = fixture()?;
        let initial = decode(&bytes)?;
        let object = object_from_frame(&bytes)?;
        let normal = serde_json::to_string(&preserved_normal_save()).map_err(|e| e.to_string())?;
        self.eval(&format!(
            r#"await new Promise((resolve, reject) => {{
          localStorage.setItem("crabscape.save", {normal:?});
          localStorage.setItem("crabscape-style", "ink");
          const request = indexedDB.open({DATABASE:?}, 1);
          request.onupgradeneeded = () => request.result.createObjectStore("world");
          request.onerror = () => reject(String(request.error));
          request.onsuccess = () => {{
            const db = request.result, tx = db.transaction("world", "readwrite");
            const raw = {object};
            const decode = s => Uint8Array.from(atob(s), c => c.charCodeAt(0));
            const store=tx.objectStore('world');store.clear();
            store.put(decode(raw.head),'active');store.put(decode(raw.epoch),'reset_epoch');
            for(const [key,bytes] of raw.records)store.put(decode(bytes),key);
            tx.onabort = () => {{ db.close(); reject(String(tx.error)); }};
            tx.oncomplete = () => {{ db.close(); resolve(true); }};
          }};
        }})"#
        ))?;
        let started = self.open("index.html?cells")?;
        let viewport = self.eval("[innerWidth, innerHeight]")?;
        self.check(
            viewport[0].as_f64() == Some(self.o.width)
                && viewport[1].as_f64() == Some(self.o.height),
            format!(
                "cellular save regression runs at {}x{} CSS pixels",
                self.o.width, self.o.height
            ),
        );
        let loaded = started && self.attr("data-cell-save")?.as_deref() == Some("loaded");
        self.check(
            loaded,
            "the full cellular game loads the non-default player and terrain fixture",
        );
        if !loaded {
            return Ok(());
        }
        self.key("keydown", "KeyD", "d")?;
        let walked = self.wait_for(
            &format!("Number({CANVAS}.getAttribute('data-crab').split(/\\s+/)[0]) > 430"),
            15.0,
        );
        self.key("keyup", "KeyD", "d")?;
        let walked = walked?;
        self.check(
            walked,
            format!(
                "the cellular crab actually walks before saving (at {:?})",
                self.crab()?
            ),
        );
        if !walked {
            return Ok(());
        }
        self.key("keydown", "Space", " ")?;
        self.key("keydown", "KeyS", "s")?;
        let dug = self.wait_for(
            &format!("Number({CANVAS}.getAttribute('data-crab').split(/\\s+/)[1]) < -16"),
            20.0,
        );
        self.key("keyup", "KeyS", "s")?;
        self.key("keyup", "Space", " ")?;
        let dug = dug?;
        self.check(
            dug,
            format!(
                "the cellular crab digs below the original floor (at {:?})",
                self.crab()?
            ),
        );
        if !dug {
            return Ok(());
        }
        // An earlier autosave may have committed while the slow browser was
        // still walking. Wait specifically for a committed post-dig position.
        let newer = self.wait_for_commit(60.0, |saved| {
            saved.player["x"].as_f64().is_some_and(|x| x > 405.0)
                && saved.player["y"].as_f64().is_some_and(|y| y < -12.0)
        })?;
        self.check(
            newer,
            "the running game commits a newer cellular autosave through its real worker",
        );
        if !newer {
            return Ok(());
        }
        // Stop first, then read the final committed value, so another save cannot race reload expectations.
        self.go("assets/crab/crab-atlas.png")?;
        let saved = decode(
            &self
                .cellular_record()?
                .ok_or("autosave record disappeared")?,
        )?;
        let material_changes = changed_material_cells(&initial.terrain, &saved.terrain);
        let marker = terrain_has_marker(&saved.terrain);
        self.check(
            saved.tick > 17
                && saved.player["x"].as_f64().is_some_and(|x| x > 405.0)
                && saved.player["y"].as_f64().is_some_and(|y| y < -12.0)
                && material_changes > 0
                && marker,
            format!("the autosave contains the dug player, evolved material and terrain marker (tick {}, player ({}, {}), changed cells {material_changes}, marker {marker}, active bounds {:?})", saved.tick, saved.player["x"], saved.player["y"], saved.terrain.game_bounds()),
        );

        let started = self.open("index.html?cells")?;
        let loaded = started && self.attr("data-cell-save")?.as_deref() == Some("loaded");
        self.check(loaded, "the game reloads its own cellular autosave");
        if !loaded {
            return Ok(());
        }
        let replay = self.cellular_replay()?;
        let mut restored = Sim::with_one_crab(replay.start);
        let prepared = replay.prepare_start(&mut restored);
        self.check(prepared && replay.has_cell_start() && restored.tick_count() == saved.tick
            && (f64::from(replay.start.x) - saved.player["x"].as_f64().unwrap_or(f64::NAN)).abs() < 0.001
            && (f64::from(replay.start.y) - saved.player["y"].as_f64().unwrap_or(f64::NAN)).abs() < 0.001
            && restored.level.cells.as_ref().is_some_and(|world| world.checkpoint() == saved.terrain.checkpoint()),
            "the reloaded game's replay proves exact terrain, player and simulation-clock restoration");
        self.close_cellular_replay()?;
        self.check(
            self.eval("localStorage.getItem('crabscape.save')")? == normal,
            "cellular autosave and reload leave the normal game's save untouched",
        );

        let (x, y) = settings_button(self.o.width);
        self.tap(x, y, 12, true)?;
        let opened = self.wait_for(
            &format!("{CANVAS}.getAttribute('data-settings') !== 'closed'"),
            5.0,
        )?;
        let (x, y) = settings_tab(self.o.width, "game");
        self.tap(x, y, 12, true)?;
        let game = self.wait_for(
            &format!("{CANVAS}.getAttribute('data-settings') === 'Game'"),
            5.0,
        )?;
        let (x, y) = start_over_centre(self.o.width);
        self.tap(x, y, 13, true)?;
        let armed = self.wait_for(
            &format!("{CANVAS}.getAttribute('data-settings') === 'Game armed StartOver'"),
            5.0,
        )?;
        self.check(
            opened && game && armed && self.cellular_record_exists()?,
            "one Start over tap keeps the cellular checkpoint",
        );
        self.eval("(window.__cellularResetBefore = true, true)")?;
        self.tap(x, y, 14, true)?;
        let reset = self.wait_for(&format!("(() => {{ const c = {CANVAS}; return !window.__cellularResetBefore && !!c && c.getAttribute('data-cell-save') === 'fresh' && c.hasAttribute('data-crab'); }})()"), 90.0)?;
        self.check(
            reset,
            "confirmed Start over commits deletion and reloads the full game fresh",
        );
        if !reset {
            let details = self.eval(&format!("(() => {{ const c = {CANVAS}; return {{ samePage: !!window.__cellularResetBefore, settings: c?.getAttribute('data-settings'), save: c?.getAttribute('data-cell-save'), message: document.getElementById('message')?.textContent }}; }})()"))?;
            println!("     reset failure state: {details}");
            return Ok(());
        }
        let at = self.crab()?;
        // Fresh status proves startup read no old record. If another five
        // seconds elapsed on slow WebKit, a valid fresh autosave is allowed,
        // but it must not contain the previous world's rock marker.
        let record = self.cellular_record()?;
        let (old_removed, record_description) = match record {
            None => (true, "absent".to_string()),
            Some(bytes) => {
                let fresh = decode(&bytes)?;
                let marker = terrain_has_marker(&fresh.terrain);
                (
                    !marker,
                    format!(
                        "new save tick {}, player ({}, {}), old marker {marker}",
                        fresh.tick, fresh.player["x"], fresh.player["y"]
                    ),
                )
            }
        };
        self.check(old_removed, format!("reset removes the previous terrain checkpoint (record {record_description}; live crab {at:?})"));
        self.check(
            self.eval("localStorage.getItem('crabscape.save')")? == normal
                && self.eval("localStorage.getItem('crabscape-style')")? == "ink",
            "cellular reset preserves the normal save and style",
        );
        let fresh = self.cellular_replay()?;
        let mut expected = Sim::new(crabsim::Tuning::default(), Level::test_level());
        expected.enable_underwater_cells();
        if let Some((_, feel)) = fresh.feels.iter().find(|(tick, _)| *tick == 0) {
            expected.set_feel(*feel);
        }
        expected.add_crab(crabsim::PlayerId(0), crabsim::SPAWN);
        let expected_start = expected
            .crab(crabsim::PlayerId(0))
            .ok_or("missing expected fresh crab")?
            .mover
            .pos;
        self.check(
            fresh.cellular && !fresh.has_cell_start() && fresh.start == expected_start,
            format!("reset records a fresh cellular world at its actual spawn surface (recorded {:?}, expected {expected_start:?}, live {at:?})", fresh.start),
        );
        self.close_cellular_replay()?;
        // A second browsing context performs the same atomic reset while the
        // original game remains alive. Its next real worker save must be
        // fenced, and a newly loaded session must still be able to save.
        self.eval(r#"await new Promise((resolve, reject) => {
          const frame = document.createElement('iframe');
          frame.hidden = true;
          frame.onload = () => {
            const opening = frame.contentWindow.indexedDB.open('crabscape.cells', 1);
            opening.onerror = () => { frame.remove(); reject(String(opening.error)); };
            opening.onsuccess = () => {
              const db = opening.result, tx = db.transaction('world', 'readwrite');
              const store = tx.objectStore('world'), request = store.get('reset_epoch');
              request.onsuccess = () => {
                const epoch = request.result === undefined ? new Uint8Array(8) : request.result.slice();
                if (!ArrayBuffer.isView(epoch) || epoch.length !== 8) { tx.abort(); return; }
                let i = 0;
                while (i < 8 && epoch[i] === 255) { epoch[i] = 0; i++; }
                if (i === 8) { tx.abort(); return; }
                epoch[i]++;
                store.delete('active'); store.put(epoch, 'reset_epoch');
              };
              tx.onabort = () => { db.close(); frame.remove(); reject('cross-tab reset failed'); };
              tx.oncomplete = () => { db.close(); frame.remove(); resolve(true); };
            };
          };
          frame.src = 'about:blank'; document.body.appendChild(frame);
        })"#)?;
        let fenced = self.wait_for(
            &format!("{CANVAS}.getAttribute('data-cell-autosave') === 'stale-epoch'"),
            60.0,
        )?;
        self.check(
            fenced && self.cellular_record()?.is_none(),
            "an old tab cannot resurrect a cellular save reset in another browsing context",
        );
        if !fenced {
            return Ok(());
        }
        let restarted = self.open("index.html?cells")?;
        let fresh_session = restarted && self.attr("data-cell-save")?.as_deref() == Some("fresh");
        let saved_again = fresh_session && self.wait_for_commit(60.0, |_| true)?;
        self.check(
            saved_again,
            "a newly loaded cellular session saves after a cross-tab reset",
        );
        self.no_error_on_page("after cellular autosave, reload and reset")?;
        let normal_started = self.open("index.html?cells&boring")?;
        let normal_replay = if normal_started {
            Some(self.cellular_replay()?)
        } else {
            None
        };
        self.check(
            normal_replay.is_some_and(|replay| !replay.cellular && replay.start.x == 918.0),
            "boring mode restores the preserved normal save at x918 even when cells is requested",
        );
        Ok(())
    }

    pub(super) fn incompatible_player_recovery(&mut self) -> Result<()> {
        // Use the same real HTML storage context as the large-fixture setup.
        // Keep IndexedDB setup out of the image document used for readbacks.
        self.go("__fixture_setup")?;
        let old = decode(&fixture()?)?;
        let initial = Sim::new(crabsim::Tuning::default(), Level::test_level());
        let mut progress = initial.fresh_progress();
        progress.eaten.push(true); // A real current validator incompatibility.
        progress.swim_xp = 12345.0;
        let player = crabsim::SaveData {
            version: crabsim::SAVE_VERSION,
            x: 400.0,
            y: 0.0,
            memory: None,
            progress: Some(progress),
        };
        let bytes = crabsim::cell_codec::paged::prepare(&player, &old.terrain, 17)
            .and_then(|prepared| prepared.encode())
            .map_err(|error| format!("recovery fixture: {error:?}"))?;
        let object = object_from_frame(&bytes)?;
        fixture_install::install(DATABASE, &object, |script| self.eval(script))?;
        let started = self.open("index.html?cells")?;
        let recovered = started && self.attr("data-cell-save")?.as_deref() == Some("player-reset");
        self.check(
            recovered,
            "incompatible player data resets the crab into a playable world",
        );
        if !recovered {
            return Ok(());
        }
        let eaten = initial.fresh_progress().eaten.len();
        let saved = self.wait_for_commit(60.0, |saved| {
            let progress = &saved.player["progress"];
            saved.head.tick > 17
                && progress["swim_xp"].as_f64() == Some(0.0)
                && progress["eaten"].as_array().map(Vec::len) == Some(eaten)
        })?;
        self.check(
            saved,
            "reset crab autosaves compatible fresh progress through the real worker",
        );
        if !saved {
            return Ok(());
        }
        self.go("assets/crab/crab-atlas.png")?;
        let current = decode(&self.cellular_record()?.ok_or("reset autosave missing")?)?;
        self.check(
            terrain_has_marker(&current.terrain),
            "player reset preserves saved terrain marker",
        );
        let loaded = self.open("index.html?cells")?;
        self.check(
            loaded && self.attr("data-cell-save")?.as_deref() == Some("loaded"),
            "recovered autosave reloads normally without another crab reset",
        );
        Ok(())
    }

    pub(super) fn large_cellular_autosave(&mut self) -> Result<()> {
        self.go("__fixture_setup")?;
        let large = object_from_frame(&large_fixture()?)?;
        fixture_install::install(DATABASE, &large, |script| self.eval(script))?;
        println!("     committed large cellular fixture; opening game");
        let started = self.open("index.html?cells")?;
        let loaded = started && self.attr("data-cell-save")?.as_deref() == Some("loaded");
        // The first save after load may be a paging flush at the restored
        // tick 17. Wait for a commit the running simulation produced.
        let first = std::cell::Cell::new(None);
        let saved_large = loaded
            && self.wait_for_commit(180.0, |saved| {
                let at = (saved.head.revision, saved.head.tick);
                if saved.head.revision > 1 && first.get().is_none() {
                    first.set(Some(at));
                }
                saved.head.tick > 17
            })?;
        if let Some((revision, tick)) = first.get() {
            println!("     first game commit after load: revision {revision}, tick {tick}");
        }
        self.check(
            saved_large,
            "a world larger than the former browser limit loads and autosaves",
        );
        if !saved_large {
            eprintln!(
                "large world diagnostics: loaded={loaded}, save={:?}, autosave={:?}, player={:?}, page={:?}",
                self.attr("data-cell-save")?,
                self.attr("data-cell-autosave")?,
                self.committed()?
                    .map(|saved| (saved.head.revision, saved.head.tick, saved.player)),
                self.eval("document.body.innerText")?
            );
            return Ok(());
        }
        self.go("assets/crab/crab-atlas.png")?;
        let saved = decode(
            &self
                .cellular_record()?
                .ok_or("large autosave disappeared")?,
        )?;
        let chunks = crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&saved.terrain)
            .map_err(|e| format!("large saved snapshot: {e:?}"))?
            .sleeping_ids()
            .count();
        self.check(
            chunks > 256 && saved.tick > 17,
            format!(
                "large autosave retains explored terrain ({chunks} sleeping chunks, tick {})",
                saved.tick
            ),
        );
        let reloaded = self.open("index.html?cells")?;
        self.check(
            reloaded && self.attr("data-cell-save")?.as_deref() == Some("loaded"),
            "the game reloads its own large paged autosave",
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_reader_decodes_the_tick_behind_a_published_canyon_hash() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use crabsim::cell_codec::spatial::{SPATIAL_MAGIC, SpatialHead, encode_record};
        // The old JS parse read three varints after the magic: here the
        // Option tag (1), hash byte 5, and hash byte 64 as "tick 64", so the
        // large-save wait passed on a flush committed at the restored tick 17.
        let mut hash = [0; 32];
        hash[..2].copy_from_slice(&[5, 64]);
        let head = SpatialHead {
            canyon_hash: Some(hash),
            epoch: 0,
            revision: 4,
            tick: 17,
            cols: 3,
            rows: 2,
        };
        let mut raw = SPATIAL_MAGIC.to_vec();
        raw.extend(postcard::to_allocvec(&head).unwrap());
        let player = br#"{"x":400.0}"#;
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(player, 6);
        let value = |written| {
            serde_json::json!({
                "head": STANDARD.encode(&raw),
                "player": STANDARD.encode(encode_record(player, &packed, written)),
            })
        };
        let saved = committed_from(&value(4)).unwrap().unwrap();
        assert_eq!((saved.head.revision, saved.head.tick), (4, 17));
        assert_eq!(saved.player["x"], 400.0);
        assert!(
            committed_from(&value(5)).is_err(),
            "a player record newer than its head is not that head's save"
        );
        assert!(committed_from(&Value::Null).unwrap().is_none());
    }

    #[test]
    fn preserved_normal_fixture_has_a_current_valid_restore_position() {
        let saved = preserved_normal_save();
        assert_eq!(saved.version, crabsim::SAVE_VERSION);
        let mut position = crabsim::SPAWN;
        position.x = saved.x;
        position.y = saved.y;
        assert!(
            crabsim::physics::is_valid_spot(
                position,
                1.0,
                &crabsim::Tuning::default(),
                &Level::test_level()
            ),
            "normal fixture would be rejected and restore SPAWN"
        );
    }
    #[test]
    fn fixture_has_distinct_terrain_player_and_checked_paged_record() {
        let bytes = fixture().unwrap();
        let saved = decode(&bytes).unwrap();
        assert_eq!(saved.tick, 17);
        assert_eq!(saved.player["x"], 400.0);
        assert!(saved.terrain.game_rock_at(MARKER.0, MARKER.1));
        let mut damaged = bytes;
        let last = damaged.len() - 1;
        damaged[last] ^= 1;
        assert!(decode(&damaged).is_err());
    }

    #[test]
    fn large_paged_fixture_survives_object_and_native_roundtrip() {
        let frame = large_fixture().unwrap();
        let object = object_from_frame(&frame).unwrap();
        let roundtrip = frame_from_object(&object).unwrap();
        assert_eq!(frame, roundtrip);
        let restored = decode(&roundtrip).unwrap();
        assert!(
            crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&restored.terrain)
                .unwrap()
                .sleeping_ids()
                .count()
                > 256
        );
    }

    #[test]
    fn observed_dig_changes_materials_but_metadata_alone_does_not() {
        use crabsim::{Controls, PlayerId, underwater::Cell};
        let initial = decode(&fixture().unwrap()).unwrap();
        let mut metadata_only = initial.terrain.clone();
        let mut properties = metadata_only.sediment_properties(Cell::Sand).unwrap();
        properties.push_cost += 1;
        metadata_only.set_sediment_properties(Cell::Sand, properties);
        assert_ne!(metadata_only.checkpoint(), initial.terrain.checkpoint());
        assert_eq!(changed_material_cells(&initial.terrain, &metadata_only), 0);

        let mut sim = Sim::new(crabsim::Tuning::default(), Level::test_level());
        assert!(sim.restore_cellular_terrain(initial.terrain.clone(), initial.tick));
        let mut start = crabsim::SPAWN;
        start.x = 430.0;
        sim.add_crab(PlayerId(0), start);
        for _ in 0..600 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    dig_held: true,
                    down: true,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(PlayerId(0)).unwrap().mover.pos.y < -16.0 {
                break;
            }
        }
        let position = sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert!(
            position.y < -16.0,
            "fixture must support the browser's downward dig: {position:?}"
        );
        let terrain = sim.level.cells.as_ref().unwrap();
        assert!(changed_material_cells(&initial.terrain, terrain) > 0);
        assert!(terrain_has_marker(terrain));
    }

    #[test]
    fn marker_survives_a_downward_streamed_browser_checkpoint() {
        let mut saved = decode(&fixture().unwrap()).unwrap();
        let level = Level::test_level();
        assert!(saved.terrain.follow_player(&level, 525.141, -48.0));
        assert!(!saved.terrain.contains_world(MARKER.0, MARKER.1));
        assert!(!saved.terrain.game_rock_at(MARKER.0, MARKER.1));
        let player: crabsim::SaveData = serde_json::from_value(saved.player.clone()).unwrap();
        let raw = crabsim::cell_codec::paged::prepare(&player, &saved.terrain, saved.tick)
            .unwrap()
            .encode()
            .unwrap();
        let restored = crabsim::cell_codec::paged::decode(&raw).unwrap().terrain;
        let before_inspection = restored.checkpoint();
        assert!(terrain_has_marker(&restored));
        assert_eq!(restored.checkpoint(), before_inspection);

        let mut normal = World::from_level_settled(&level);
        assert!(normal.follow_player(&level, 525.141, -48.0));
        assert!(
            !terrain_has_marker(&normal),
            "normal regeneration cannot forge the saved marker"
        );
    }
}
