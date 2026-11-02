use crate::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use crabsim::{Level, underwater::World};
use serde_json::{Value, json};

pub fn large_fixture() -> Result<Vec<u8>> {
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
    // A large terrain archive alone missed the real-world save failure:
    // explored map memory outgrew the former 64 KiB player-record limit.
    let mut memory = crabsim::Memory::new(&level, &crabsim::Tuning::default().memory).to_save();
    memory.now = 1_000_000;
    let mut remembered = Vec::new();
    for i in 0..memory.cols * memory.rows {
        remembered.extend_from_slice(&[u8::from(i != 0), 8, 192, 132, 61]);
    }
    memory.cells = STANDARD.encode(remembered);
    let player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: 400.0,
        y: 0.0,
        memory: Some(memory),
        progress: None,
    };
    if serde_json::to_vec(&player)
        .map_err(|e| e.to_string())?
        .len()
        <= 264_201
    {
        return Err("fixture must exceed the reported desktop character size".into());
    }
    crabsim::cell_codec::paged::prepare(&player, &terrain, 17)
        .and_then(|p| p.encode())
        .map_err(|e| format!("large fixture: {e:?}"))
}

pub fn verify_full_memory(player: &crabsim::SaveData) -> Result<()> {
    let saved = player
        .memory
        .as_ref()
        .ok_or("saved exploration is missing")?;
    let memory = crabsim::Memory::from_save(
        saved,
        &Level::test_level(),
        &crabsim::Tuning::default().memory,
    )
    .ok_or("saved exploration is invalid")?;
    if saved.cells.len() <= 64 * 1024
        || (0..memory.cols * memory.rows).any(|i| memory.visits(i) != 8)
    {
        return Err("large character lost remembered map cells".into());
    }
    Ok(())
}

pub fn object_from_frame(bytes: &[u8]) -> Result<Value> {
    if bytes.len() < 28 || &bytes[..8] != b"CRABPG02" {
        return Err("invalid frame".into());
    }
    let n = |at: usize| {
        u32::from_le_bytes(bytes[at..at + 4].try_into().expect("header checked")) as usize
    };
    let (player_len, index_len, count) = (n(16), n(20), n(24));
    let mut at = 28usize;
    let mut take = |size: usize| -> Result<&[u8]> {
        let end = at
            .checked_add(size)
            .filter(|&end| end <= bytes.len())
            .ok_or("truncated frame")?;
        let part = &bytes[at..end];
        at = end;
        Ok(part)
    };
    let player = STANDARD.encode(take(player_len)?);
    let index = STANDARD.encode(take(index_len)?);
    let mut pages = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u32::from_le_bytes(take(4)?.try_into().map_err(|_| "bad page length")?) as usize;
        pages.push(STANDARD.encode(take(len)?));
    }
    if at != bytes.len() {
        return Err("trailing frame".into());
    }
    Ok(
        json!({"format":"CRABPG02", "tick":STANDARD.encode(&bytes[8..16]), "player":player, "index":index, "pages":pages}),
    )
}

pub fn frame_from_object(value: &Value) -> Result<Vec<u8>> {
    if value["format"] != "CRABPG02" {
        return Err("invalid object format".into());
    }
    let get = |key: &str| -> Result<Vec<u8>> {
        STANDARD
            .decode(value[key].as_str().ok_or("missing object field")?)
            .map_err(|e| e.to_string())
    };
    let tick = u64::from_le_bytes(get("tick")?.try_into().map_err(|_| "bad tick")?);
    let player = get("player")?;
    let index = get("index")?;
    let pages = value["pages"]
        .as_array()
        .ok_or("missing pages")?
        .iter()
        .map(|p| {
            STANDARD
                .decode(p.as_str().ok_or("bad page")?)
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    crabsim::cell_codec::paged::pack(tick, &player, &index, &pages)
        .map_err(|e| format!("invalid frame: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_fixture_survives_browser_object_framing_exactly() {
        let frame = large_fixture().unwrap();
        let object = object_from_frame(&frame).unwrap();
        let repacked = frame_from_object(&object).unwrap();
        assert_eq!(frame, repacked);
        let restored = crabsim::cell_codec::paged::decode(&repacked).unwrap();
        assert_eq!(restored.sim_tick, 17);
        let snapshot =
            crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&restored.terrain)
                .unwrap();
        assert!(snapshot.sleeping_ids().count() > 256);
    }
}
