//! Full game regression, using the same WebKit session and phone frame as the other checks.
use base64::{Engine, engine::general_purpose::STANDARD};
use crabsim::{Level, Replay, Sim, underwater::World};
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
        self.hold(0.6, false)?;
        self.key("keyup", "KeyD", "d")?;
        let newer = self.wait_for(&format!("({READ}) !== {encoded:?}"), 60.0)?;
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
        self.check(
            saved.tick > 17
                && saved.player["x"].as_f64().is_some_and(|x| x > 405.0)
                && saved.terrain.game_rock_at(MARKER.0, MARKER.1),
            "the autosave contains advanced player/tick and the non-default terrain marker",
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
        self.check(
            self.cellular_record()?.is_none()
                && at.is_some_and(|(x, y, _)| (x - 160.0).abs() < 3.0 && y.abs() < 3.0),
            "reset removes the previous checkpoint and returns the crab to spawn",
        );
        self.check(
            self.eval("localStorage.getItem('crabscape.save')")? == normal
                && self.eval("localStorage.getItem('crabscape-style')")? == "ink",
            "cellular reset preserves the normal save and style",
        );
        let fresh = self.cellular_replay()?;
        self.check(
            fresh.cellular && !fresh.has_cell_start(),
            "reset records a fresh cellular world instead of reusing saved terrain",
        );
        self.no_error_on_page("after cellular autosave, reload and reset")?;
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
}
