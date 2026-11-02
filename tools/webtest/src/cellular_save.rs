//! Full game regression, using the same WebKit session and phone frame as the other checks.
use base64::{Engine, engine::general_purpose::STANDARD};
use crabsim::{
    Level, Replay, Sim,
    underwater::{GAME_CELL, HEIGHT, WIDTH, World},
};
use serde_json::{Value, json};

use crate::{CANVAS, Test, driver::Result, settings_button, settings_tab, start_over_centre};

const DATABASE: &str = "crabscape.cells";
const MARKER: (f32, f32) = (1000.0, 700.0);
const HEADER: usize = 28;

struct Saved {
    tick: u64,
    player: Value,
    terrain: World,
}

fn checksum(tick: u64, player: &[u8], raw: &[u8]) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    hash.update(&tick.to_le_bytes());
    hash.update(&(player.len() as u32).to_le_bytes());
    hash.update(&(raw.len() as u32).to_le_bytes());
    hash.update(player);
    hash.update(raw);
    hash.finalize()
}

/// A stable rock cell absent from the normal level catches player-only restoration.
/// This is storage fixture setup, not a gameplay mutation or a test-only game hook.
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
    let raw = crabsim::underwater::encode_browser_checkpoint(&terrain)
        .ok_or("cellular fixture exceeds the browser checkpoint budget")?;
    let player = serde_json::to_vec(&json!({"version": 2, "x": 400.0, "y": 0.0}))
        .map_err(|e| e.to_string())?;
    let tick = 17u64;
    let mut bytes = b"CRABCEL2".to_vec();
    bytes.extend_from_slice(&tick.to_le_bytes());
    bytes.extend_from_slice(&(player.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&checksum(tick, &player, &raw).to_le_bytes());
    bytes.extend_from_slice(&player);
    bytes.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
    Ok(bytes)
}

fn old_opening_fixture() -> Result<Vec<u8>> {
    let terrain: World = postcard::from_bytes(include_bytes!(
        "../../../crates/sim/assets/opening-settled.postcard"
    ))
    .map_err(|error| error.to_string())?;
    if terrain.game_pebbles().is_empty() {
        return Err("historical opening must contain pebbles".into());
    }
    let raw =
        crabsim::underwater::encode_browser_checkpoint(&terrain).ok_or("opening too large")?;
    let player = serde_json::to_vec(&json!({"version":2,"x":200.25,"y":40.5})).unwrap();
    let tick = 17u64;
    let mut bytes = b"CRABCEL2".to_vec();
    bytes.extend_from_slice(&tick.to_le_bytes());
    bytes.extend_from_slice(&(player.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&checksum(tick, &player, &raw).to_le_bytes());
    bytes.extend_from_slice(&player);
    bytes.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Saved> {
    if bytes.len() < HEADER || !bytes.starts_with(b"CRABCEL2") {
        return Err("the game did not write a complete v2 cellular save".into());
    }
    let tick = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| "bad save tick")?);
    let player_len =
        u32::from_le_bytes(bytes[16..20].try_into().map_err(|_| "bad player size")?) as usize;
    let raw_len =
        u32::from_le_bytes(bytes[20..24].try_into().map_err(|_| "bad terrain size")?) as usize;
    if bytes.len() > 8 * 1024 * 1024 || raw_len > 12 * 1024 * 1024 || player_len > 1024 * 1024 {
        return Err("the autosave exceeds its browser reload budget".into());
    }
    let end = HEADER
        .checked_add(player_len)
        .filter(|&n| n <= bytes.len())
        .ok_or("truncated player")?;
    let player_bytes = &bytes[HEADER..end];
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[end..], raw_len)
        .map_err(|e| format!("invalid autosave compression: {e:?}"))?;
    let expected = u32::from_le_bytes(bytes[24..28].try_into().map_err(|_| "bad checksum")?);
    if raw.len() != raw_len || checksum(tick, player_bytes, &raw) != expected {
        return Err("the autosave player/header/terrain checksum does not match".into());
    }
    Ok(Saved {
        tick,
        player: serde_json::from_slice(player_bytes).map_err(|e| e.to_string())?,
        terrain: crabsim::underwater::decode_browser_checkpoint(&raw)
            .ok_or("invalid saved terrain")?,
    })
}

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

// Resolve on transaction completion, rather than only on request success.
const READ: &str = r#"await new Promise((resolve, reject) => {
  const opening = indexedDB.open("crabscape.cells", 1);
  opening.onupgradeneeded = () => opening.result.createObjectStore("world");
  opening.onerror = () => reject(String(opening.error));
  opening.onsuccess = () => {
    const db = opening.result, tx = db.transaction("world", "readonly");
    const request = tx.objectStore("world").get("active");
    tx.onabort = () => { db.close(); reject(String(tx.error)); };
    tx.oncomplete = () => {
      db.close();
      if (request.result === undefined) { resolve(null); return; }
      const bytes = request.result, pieces = [];
      for (let i = 0; i < bytes.length; i += 32768)
        pieces.push(String.fromCharCode(...bytes.subarray(i, i + 32768)));
      resolve(btoa(pieces.join("")));
    };
  };
})"#;

impl Test<'_> {
    fn cellular_record(&self) -> Result<Option<Vec<u8>>> {
        self.eval(READ)?
            .as_str()
            .map(|s| STANDARD.decode(s).map_err(|e| e.to_string()))
            .transpose()
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
        let encoded = STANDARD.encode(&bytes);
        let normal = json!({"version": 2, "x": 918.0, "y": 0.0}).to_string();
        self.eval(&format!(r#"await new Promise((resolve, reject) => {{
          localStorage.setItem("crabscape.save", {normal:?});
          localStorage.setItem("crabscape-style", "ink");
          const request = indexedDB.open({DATABASE:?}, 1);
          request.onupgradeneeded = () => request.result.createObjectStore("world");
          request.onerror = () => reject(String(request.error));
          request.onsuccess = () => {{
            const db = request.result, tx = db.transaction("world", "readwrite");
            tx.objectStore("world").put(Uint8Array.from(atob({encoded:?}), c => c.charCodeAt(0)), "active");
            tx.objectStore("world").delete("reset_epoch"); // Exercise legacy records without an epoch.
            tx.onabort = () => {{ db.close(); reject(String(tx.error)); }};
            tx.oncomplete = () => {{ db.close(); resolve(true); }};
          }};
        }})"#))?;
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
        self.key("keydown", "KeyC", "c")?;
        self.key("keydown", "KeyS", "s")?;
        let dug = self.wait_for(
            &format!("Number({CANVAS}.getAttribute('data-crab').split(/\\s+/)[1]) < -16"),
            20.0,
        );
        self.key("keyup", "KeyS", "s")?;
        self.key("keyup", "KeyC", "c")?;
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
        let newer = self.wait_for(
            &format!(
                r#"await (async () => {{
          const record = {READ};
          if (!record) return false;
          const bytes = atob(record);
          if (bytes.slice(0, 8) !== "CRABCEL2") return false;
          const size = (bytes.charCodeAt(16) | bytes.charCodeAt(17) << 8 |
            bytes.charCodeAt(18) << 16 | bytes.charCodeAt(19) << 24) >>> 0;
          const player = JSON.parse(bytes.slice(28, 28 + size));
          return player.x > 405 && player.y < -12;
        }})()"#
            ),
            60.0,
        )?;
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
        self.eval("(document.getElementById('replay-box').hidden = true, true)")?;
        self.check(
            self.eval("localStorage.getItem('crabscape.save')")? == normal,
            "cellular autosave and reload leave the normal game's save untouched",
        );

        let (x, y) = settings_button(self.o.width);
        self.tap(x, y, 12, true)?;
        let (x, y) = settings_tab(self.o.width, "game");
        self.tap(x, y, 12, true)?;
        let (x, y) = start_over_centre(self.o.width);
        self.tap(x, y, 13, true)?;
        self.check(
            self.cellular_record()?.is_some(),
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
        self.eval("(document.getElementById('replay-box').hidden = true, true)")?;
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
        let saved_again = fresh_session
            && self.wait_for(&format!("await (async () => ({READ}) !== null)()"), 60.0)?;
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
            normal_replay.is_some_and(|replay| !replay.cellular),
            "boring mode opens the normal game even when cells is requested",
        );
        self.go("assets/crab/crab-atlas.png")?;
        let opening = old_opening_fixture()?;
        let encoded = STANDARD.encode(&opening);
        self.eval(&format!(r#"await new Promise((resolve,reject)=>{{
          const r=indexedDB.open({DATABASE:?},1);
          r.onsuccess=()=>{{ const db=r.result,tx=db.transaction('world','readwrite'),s=tx.objectStore('world');
            s.put(Uint8Array.from(atob({encoded:?}),c=>c.charCodeAt(0)),'active');
            for(const k of ['reset_epoch','cleanup_marker','cleanup_backup']) s.delete(k);
            tx.oncomplete=()=>{{db.close();resolve(true)}};tx.onabort=()=>reject('fixture write failed'); }};
          r.onerror=()=>reject('fixture open failed');
        }})"#))?;
        let migrated = self.open("index.html?cells")?;
        let saved = self
            .cellular_record()?
            .ok_or("opening migration lost save")?;
        let state = decode(&saved)?;
        self.check(
            migrated
                && state.terrain.game_pebbles().is_empty()
                && state.tick == 17
                && state.player["x"] == 200.25
                && state.player["y"] == 40.5,
            "real startup removes the saved opening pebble bed and preserves player/tick",
        );
        self.check(self.eval(&format!(r#"await new Promise((resolve,reject)=>{{
          const r=indexedDB.open({DATABASE:?},1);r.onerror=()=>reject('open');r.onsuccess=()=>{{
            const db=r.result,tx=db.transaction('world','readonly'),s=tx.objectStore('world');
            const b=s.get('cleanup_backup'),m=s.get('cleanup_marker'),e=s.get('reset_epoch');
            tx.oncomplete=()=>{{ const v=b.result,expected=atob({encoded:?});resolve(!!m.result && !!e.result && e.result[0]===1 && !!v && v.length===expected.length && v.every((c,i)=>c===expected.charCodeAt(i)));db.close(); }};
          }};
        }})"#))?.as_bool()==Some(true), "migration preserves exact backup and fences the previous epoch");
        let replay = self.cellular_replay()?;
        let mut restored = Sim::with_one_crab(replay.start);
        self.check(
            replay.prepare_start(&mut restored)
                && restored
                    .level
                    .cells
                    .as_ref()
                    .is_some_and(|w| w.game_pebbles().is_empty()),
            "migrated startup exports the new sandy replay checkpoint",
        );
        let migrated_bytes = STANDARD.encode(&saved);
        let advanced = self.wait_for(
            &format!("await (async () => ({READ}) !== {migrated_bytes:?})()"),
            30.0,
        )?;
        self.check(advanced, "migrated world continues autosaving");
        let reloaded = self.open("index.html?cells")?;
        self.check(
            reloaded && self.attr("data-cell-save")?.as_deref() == Some("loaded"),
            "migrated autosave reloads with a matching marker",
        );
        let (x, y) = settings_button(self.o.width);
        self.tap(x, y, 12, true)?;
        let (x, y) = settings_tab(self.o.width, "game");
        self.tap(x, y, 12, true)?;
        let (x, y) = start_over_centre(self.o.width);
        self.tap(x, y, 13, true)?;
        self.tap(x, y, 14, true)?;
        self.check(
            self.wait_for(
                &format!("{CANVAS}.getAttribute('data-cell-save') === 'fresh'"),
                90.0,
            )?,
            "reset clears the migrated save and reloads fresh",
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_has_distinct_terrain_player_and_checked_v2_envelope() {
        let bytes = fixture().unwrap();
        let saved = decode(&bytes).unwrap();
        assert_eq!(saved.tick, 17);
        assert_eq!(saved.player["x"], 400.0);
        assert!(saved.terrain.game_rock_at(MARKER.0, MARKER.1));
        let mut damaged = bytes;
        damaged[24] ^= 1;
        assert!(decode(&damaged).is_err());
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
        let raw = crabsim::underwater::encode_browser_checkpoint(&saved.terrain).unwrap();
        let restored = crabsim::underwater::decode_browser_checkpoint(&raw).unwrap();
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
