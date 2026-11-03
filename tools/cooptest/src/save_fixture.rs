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
    memory.cells.clear();
    for y in 0..memory.rows.div_ceil(crabsim::memory::MEMORY_CHUNK_SIDE) {
        for x in 0..memory.cols.div_ceil(crabsim::memory::MEMORY_CHUNK_SIDE) {
            memory.cells.push(crabsim::memory::MemoryChunk {
                x,
                y,
                cells: std::sync::Arc::new(
                    (0..1024)
                        .map(|seen| crabsim::memory::MemoryCell { visits: 8, seen })
                        .collect(),
                ),
            });
        }
    }
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
    if saved.cells.is_empty()
        || (0..memory.cols * memory.rows).any(|i| memory.visits(u64::from(i)) != 8)
    {
        return Err("large character lost remembered map cells".into());
    }
    Ok(())
}

#[cfg(test)]
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

pub fn spatial_object_from_frame(bytes: &[u8]) -> Result<Value> {
    use crabsim::cell_codec::{
        paged,
        spatial::{SPATIAL_MAGIC, SpatialHead, encode_record},
    };
    let restored = paged::decode(bytes).map_err(|e| format!("fixture decode: {e:?}"))?;
    let snapshot =
        crabsim::underwater::frozen_chunks::FrozenChunks::capture_active(&restored.terrain)
            .map_err(|e| format!("fixture capture: {e:?}"))?;
    let (cols, rows) = snapshot.active_dimensions();
    let head = SpatialHead {
        canyon_hash: None,
        epoch: 0,
        revision: 1,
        tick: restored.sim_tick,
        cols: cols as u16,
        rows: rows as u16,
    };
    let mut header = SPATIAL_MAGIC.to_vec();
    header.extend(postcard::to_allocvec(&head).map_err(|e| e.to_string())?);
    let mut records = Vec::new();
    let mut add = |kind: &str, x: i32, y: i32, raw: Vec<u8>| {
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        records.push(json!([
            ["00000000000000000000", kind, x, y, "00000000000000000000"],
            STANDARD.encode(encode_record(&raw, &packed, 1))
        ]));
    };
    let mut player = restored.player;
    let memory = player
        .memory
        .as_mut()
        .map(|m| {
            m.complete = false;
            std::mem::take(&mut m.cells)
        })
        .unwrap_or_default();
    add(
        "player",
        0,
        0,
        serde_json::to_vec(&player).map_err(|e| e.to_string())?,
    );
    add(
        "metadata",
        0,
        0,
        snapshot
            .metadata()
            .map_err(|e| format!("metadata: {e:?}"))?,
    );
    for y in 0..rows {
        for x in 0..cols {
            add(
                "active",
                x as i32,
                y as i32,
                snapshot
                    .active(x, y)
                    .map_err(|e| format!("active: {e:?}"))?,
            );
        }
    }
    for cut in restored.terrain.dirty_sleeping() {
        add(
            "terrain",
            cut.key.0,
            cut.key.1,
            cut.encode().map_err(|e| e.to_string())?,
        );
    }
    for page in memory {
        add(
            "memory",
            page.x as i32,
            page.y as i32,
            postcard::to_allocvec(&page).map_err(|e| e.to_string())?,
        );
    }
    Ok(
        json!({"format":"CRABSP05","head":STANDARD.encode(header),"epoch":STANDARD.encode(0u64.to_le_bytes()),"records":records}),
    )
}

pub fn frame_from_object(value: &Value) -> Result<Vec<u8>> {
    if value["format"] == "CRABSP05" {
        return spatial_frame(value);
    }
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

/// Read-only portable capture for browser assertions. This deliberately
/// materializes the saved archive; periodic game saving never calls it.
pub const CAPTURE_SCRIPT: &str = r#"new Promise((resolve,reject)=>{
  const parts=location.pathname.split('/').filter(Boolean);let slug=null;
  for(let i=0;i<parts.length;i++){
    if(/^pr-\d+$/.test(parts[i])){slug=parts[i];break;}
    if(parts[i]==='branches'&&parts[i+1]){slug='branches-'+parts[i+1];break;}
  }
  if(slug)slug=slug.replace(/[^a-zA-Z0-9_-]/g,'-').toLowerCase();
  const opening=indexedDB.open(slug?'crabscape@'+slug+'@.cells':'crabscape.cells',1);
  opening.onerror=()=>reject(String(opening.error));
  opening.onsuccess=()=>{
    const db=opening.result,tx=db.transaction('world','readonly'),store=tx.objectStore('world');
    const keys=store.getAllKeys(),values=store.getAll();
    tx.onabort=()=>{db.close();reject(String(tx.error));};
    tx.oncomplete=()=>{
      db.close();const at=keys.result.indexOf('active'),raw=at<0?undefined:values.result[at];
      if(raw===undefined){resolve(null);return;}
      const encode=bytes=>{const pieces=[];for(let i=0;i<bytes.length;i+=32768)pieces.push(String.fromCharCode(...bytes.subarray(i,i+32768)));return btoa(pieces.join(''));};
      if(raw.format==='CRABPG02'){resolve({format:raw.format,tick:encode(raw.tick),player:encode(raw.player),index:encode(raw.index),pages:raw.pages.map(encode)});return;}
      if(!(raw instanceof Uint8Array)){resolve(null);return;}
      const epoch=keys.result.indexOf('reset_epoch');
      const records=[];
      for(let i=0;i<keys.result.length;i++){
        const k=keys.result[i],v=values.result[i];
        if(Array.isArray(k)&&v instanceof Uint8Array)records.push([k,encode(v)]);
      }
      resolve({format:'CRABSP05',head:encode(raw),epoch:encode(epoch<0?new Uint8Array(8):values.result[epoch]),records});
    };
  };
})"#;

fn spatial_frame(value: &Value) -> Result<Vec<u8>> {
    use crabsim::{
        cell_codec::{paged, spatial::SpatialHead},
        underwater::{SleepingCut, frozen_chunks::ActiveRestore},
    };
    use std::collections::BTreeMap;
    let decode = |value: &Value| {
        STANDARD
            .decode(value.as_str().ok_or("missing spatial bytes")?)
            .map_err(|e| e.to_string())
    };
    let raw = decode(&value["head"])?;
    if !raw.starts_with(b"CRABSP05") {
        return Err("invalid spatial head".into());
    }
    let head: SpatialHead = postcard::from_bytes(&raw[8..]).map_err(|e| e.to_string())?;
    let epoch = u64::from_le_bytes(
        decode(&value["epoch"])?
            .try_into()
            .map_err(|_| "invalid spatial epoch")?,
    );
    if epoch != head.epoch {
        return Err("spatial capture crossed a reset".into());
    }
    let mut records = BTreeMap::<(String, i32, i32), Vec<u8>>::new();
    for record in value["records"]
        .as_array()
        .ok_or("missing spatial records")?
    {
        let key = record[0].as_array().ok_or("invalid spatial key")?;
        if key.len() != 5 {
            return Err("invalid spatial key length".into());
        }
        if key[0]
            .as_str()
            .ok_or("invalid epoch")?
            .parse::<u64>()
            .map_err(|e| e.to_string())?
            != epoch
        {
            continue;
        }
        let coord = |at: usize| {
            key[at]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or("invalid coordinate")
        };
        records.insert(
            (
                key[1].as_str().ok_or("invalid kind")?.to_owned(),
                coord(2)?,
                coord(3)?,
            ),
            decode(&record[1])?,
        );
    }
    let page = |kind: &str, x: i32, y: i32| -> Result<Vec<u8>> {
        let bytes = records
            .get(&(kind.to_owned(), x, y))
            .ok_or("missing spatial record")?;
        crabsim::cell_codec::spatial::decode_record(bytes, head.revision, 4 * 1024 * 1024)
            .map_err(|e| format!("bad spatial record: {e:?}"))
    };
    let mut builder =
        ActiveRestore::new(&page("metadata", 0, 0)?).map_err(|e| format!("metadata: {e:?}"))?;
    for y in 0..head.rows {
        for x in 0..head.cols {
            builder
                .active(&page("active", i32::from(x), i32::from(y))?)
                .map_err(|e| format!("active: {e:?}"))?;
        }
    }
    let mut terrain = builder.finish().map_err(|e| format!("terrain: {e:?}"))?;
    let mut player: crabsim::SaveData =
        serde_json::from_slice(&page("player", 0, 0)?).map_err(|e| e.to_string())?;
    for (kind, x, y) in records.keys() {
        if kind == "terrain" {
            let cut = SleepingCut::decode(&page(kind, *x, *y)?).map_err(|e| e.to_string())?;
            if cut.key != (*x, *y) {
                return Err("terrain coordinate mismatch".into());
            }
            terrain.restore_sleeping_cut(cut)?;
        } else if kind == "memory" {
            let chunk: crabsim::memory::MemoryChunk =
                postcard::from_bytes(&page(kind, *x, *y)?).map_err(|e| e.to_string())?;
            if (chunk.x as i32, chunk.y as i32) != (*x, *y) {
                return Err("memory coordinate mismatch".into());
            }
            player
                .memory
                .as_mut()
                .ok_or("memory metadata missing")?
                .cells
                .push(chunk);
        }
    }
    if let Some(memory) = &mut player.memory {
        memory.complete = true;
    }
    paged::prepare(&player, &terrain, head.tick)
        .and_then(|capture| capture.encode())
        .map_err(|e| format!("spatial portable capture: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_spatial_fixture_roundtrips_all_terrain_and_personal_pages() {
        let frame = large_fixture().unwrap();
        let mut original = crabsim::cell_codec::paged::decode(&frame).unwrap();
        let object = spatial_object_from_frame(&frame).unwrap();
        let mut restored =
            crabsim::cell_codec::paged::decode(&frame_from_object(&object).unwrap()).unwrap();
        assert_eq!(original.terrain.checkpoint(), restored.terrain.checkpoint());
        original
            .player
            .memory
            .as_mut()
            .unwrap()
            .cells
            .sort_by_key(|p| (p.x, p.y));
        restored
            .player
            .memory
            .as_mut()
            .unwrap()
            .cells
            .sort_by_key(|p| (p.x, p.y));
        assert!(
            original.player.memory == restored.player.memory,
            "portable memory differs"
        );
    }
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

/// A real production-head fixture with five Canyon owner pages and two
/// generation-keyed claims whose recipients are currently offline.
pub fn canyon_fixture() -> Result<Value> {
    use crabsim::{
        canyon_drift::{
            DriftProfile,
            run::{ANCHOR, CanyonRun, DriftClaimId, Grant, GrantPhase, SlotPhase},
        },
        cell_codec::{
            paged,
            spatial::{CanyonIndex, SalvageIndex},
        },
    };
    let level = Level::test_level();
    let mut terrain = World::from_level_settled(&level);
    if !terrain.follow_player(&level, 6385.0, -3160.0) {
        return Err("Canyon fixture could not load its active terrain".into());
    }
    let player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: 6385.0,
        y: -3160.0,
        memory: None,
        progress: None,
    };
    let frame = paged::prepare(&player, &terrain, 120)
        .and_then(|save| save.encode())
        .map_err(|e| format!("Canyon fixture: {e:?}"))?;
    let mut value = spatial_object_from_frame(&frame)?;
    let mut run = CanyonRun::new(71);
    for (index, slot) in run.slots.iter_mut().enumerate() {
        slot.generation = 7 + index as u32;
        slot.revision = 11;
        slot.location.x = 7000.0 + index as f32 * 512.0;
        slot.location.y = -3190.0;
        slot.phase = SlotPhase::Grant(Grant {
            id: DriftClaimId {
                world: 71,
                anchor: ANCHOR,
                slot: index as u8,
                generation: slot.generation,
            },
            player: crabsim::PlayerId(80 + index as u32),
            identity: 9000 + index as u64,
            profile: DriftProfile::Flat,
            phase: GrantPhase::Requested,
        });
    }
    run.catcher.position.x = 8050.0;
    run.catcher.position.y = -3190.0;
    run.practice.position.x = 8700.0;
    run.practice.position.y = -3190.0;
    let records = run.records();
    if records.len() != 5 {
        return Err("Canyon fixture must exercise all five page coordinates".into());
    }
    let index = CanyonIndex::new(&records).map_err(|e| format!("Canyon index: {e:?}"))?;
    append_spatial(
        &mut value,
        "canyon_index",
        (0, 0),
        &postcard::to_allocvec(&index).map_err(|e| e.to_string())?,
    )?;
    for record in &records {
        append_spatial(
            &mut value,
            "canyon",
            record.region_id,
            &postcard::to_allocvec(record).map_err(|e| e.to_string())?,
        )?;
    }
    let shellwash = crabsim::salvage::Shellwash {
        world_id: 71,
        ..Default::default()
    };
    let salvage = shellwash.records();
    append_spatial(
        &mut value,
        "salvage_index",
        (0, 0),
        &postcard::to_allocvec(
            &SalvageIndex::new(&salvage).map_err(|e| format!("salvage index: {e:?}"))?,
        )
        .map_err(|e| e.to_string())?,
    )?;
    for record in salvage {
        append_spatial(
            &mut value,
            "salvage",
            record.region_id,
            &postcard::to_allocvec(&record).map_err(|e| e.to_string())?,
        )?;
    }
    Ok(value)
}
fn append_spatial(value: &mut Value, kind: &str, coordinate: (i32, i32), raw: &[u8]) -> Result<()> {
    use crabsim::cell_codec::spatial::encode_record;
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(raw, 1);
    if kind == "canyon_index" {
        use crabsim::cell_codec::spatial::{SPATIAL_MAGIC, SpatialHead};
        let header = STANDARD
            .decode(value["head"].as_str().ok_or("missing fixture head")?)
            .map_err(|e| e.to_string())?;
        let mut head: SpatialHead =
            postcard::from_bytes(&header[8..]).map_err(|e| e.to_string())?;
        head.canyon_hash = Some(crabsim::cell_codec::paged::hash(raw));
        let mut header = SPATIAL_MAGIC.to_vec();
        header.extend(postcard::to_allocvec(&head).map_err(|e| e.to_string())?);
        value["head"] = Value::String(STANDARD.encode(header));
    }
    value["records"]
        .as_array_mut()
        .ok_or("missing spatial rows")?
        .push(json!([
            [
                "00000000000000000000",
                kind,
                coordinate.0,
                coordinate.1,
                "00000000000000000000"
            ],
            STANDARD.encode(encode_record(raw, &packed, 1)),
        ]));
    Ok(())
}
pub fn canyon_from_object(
    value: &Value,
) -> Result<Option<Vec<crabsim::canyon_drift::record::CanyonRegionRecord>>> {
    use crabsim::cell_codec::spatial::{
        CANYON_INDEX_LIMIT, CANYON_RECORD_LIMIT, CanyonIndex, SPATIAL_MAGIC, SpatialHead,
        decode_canyon, decode_record,
    };
    use std::collections::BTreeMap;
    if value.is_null() {
        return Ok(None);
    }
    let header = STANDARD
        .decode(value["head"].as_str().ok_or("missing spatial head")?)
        .map_err(|e| e.to_string())?;
    if !header.starts_with(SPATIAL_MAGIC) {
        return Err("invalid Canyon spatial head format".into());
    }
    let head: SpatialHead = postcard::from_bytes(&header[8..]).map_err(|e| e.to_string())?;
    let mut index = None;
    let mut pages = BTreeMap::new();
    for entry in value["records"].as_array().ok_or("missing spatial rows")? {
        let key = entry[0].as_array().ok_or("invalid coordinate key")?;
        if key.len() != 5 {
            return Err("invalid coordinate key size".into());
        }
        let epoch = key[0]
            .as_str()
            .ok_or("missing epoch")?
            .parse::<u64>()
            .map_err(|e| e.to_string())?;
        if epoch != head.epoch {
            continue;
        }
        let kind = key[1].as_str().ok_or("missing namespace")?;
        if kind != "canyon" && kind != "canyon_index" {
            continue;
        }
        if key[4].as_str().and_then(|v| v.parse::<u64>().ok()) != Some(0) {
            return Err("Canyon retained historical coordinate slots".into());
        }
        let coordinate = (
            i32::try_from(key[2].as_i64().ok_or("missing x")?).map_err(|e| e.to_string())?,
            i32::try_from(key[3].as_i64().ok_or("missing y")?).map_err(|e| e.to_string())?,
        );
        let bytes = STANDARD
            .decode(entry[1].as_str().ok_or("missing record bytes")?)
            .map_err(|e| e.to_string())?;
        let raw = decode_record(
            &bytes,
            head.revision,
            if kind == "canyon" {
                CANYON_RECORD_LIMIT
            } else {
                CANYON_INDEX_LIMIT
            },
        )
        .map_err(|e| format!("Canyon row: {e:?}"))?;
        if kind == "canyon_index" {
            if coordinate != (0, 0) || index.is_some() {
                return Err("Canyon has extra indexes".into());
            }
            if head.canyon_hash != Some(crabsim::cell_codec::paged::hash(&raw)) {
                return Err("Canyon index digest mismatch".into());
            }
            index = Some(postcard::from_bytes::<CanyonIndex>(&raw).map_err(|e| e.to_string())?);
        } else if pages.insert(coordinate, raw).is_some() {
            return Err("duplicate Canyon page".into());
        }
    }
    let Some(index) = index else {
        if head.canyon_hash.is_some() || !pages.is_empty() {
            return Err("orphan Canyon pages".into());
        }
        return Ok(None);
    };
    let keys = index.keys().map_err(|e| format!("Canyon keys: {e:?}"))?;
    if keys.len() != pages.len() || pages.len() > 5 {
        return Err("Canyon page count exceeds its fixed index".into());
    }
    let records = keys
        .iter()
        .map(|key| {
            decode_canyon(pages.get(key).ok_or("missing Canyon owner page")?, *key)
                .map_err(|e| format!("Canyon record: {e:?}"))
        })
        .collect::<Result<Vec<_>>>()?;
    if crabsim::canyon_drift::run::CanyonRun::from_records(&records).is_none() {
        return Err("incomplete Canyon owners".into());
    }
    Ok(Some(records))
}

#[cfg(test)]
mod canyon_tests {
    use super::*;
    #[test]
    fn five_page_canyon_fixture_round_trips_and_rejects_orphan_history() {
        let mut fixture = canyon_fixture().unwrap();
        let records = canyon_from_object(&fixture).unwrap().unwrap();
        assert_eq!(records.len(), 5);
        let run = crabsim::canyon_drift::run::CanyonRun::from_records(&records).unwrap();
        assert_eq!(run.slots[0].generation, 7);
        assert_eq!(run.slots[1].generation, 8);
        let raw = postcard::to_allocvec(&records[0]).unwrap();
        append_spatial(&mut fixture, "canyon", (9999, 0), &raw).unwrap();
        assert!(canyon_from_object(&fixture).is_err());
    }
}
