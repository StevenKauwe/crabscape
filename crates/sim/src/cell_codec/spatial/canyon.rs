//! Fixed identity index for the independently owned Canyon pages.
use super::{Capture, Error};
use crate::canyon_drift::{record::CanyonRegionRecord, run::CanyonRun};

pub const CANYON_RECORD_LIMIT: usize = 4096;
pub const CANYON_INDEX_LIMIT: usize = 80;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CanyonIndex {
    count: u8,
    coordinates: [(i32, i32); 5],
}
impl CanyonIndex {
    pub fn new(records: &[CanyonRegionRecord]) -> Result<Self, Error> {
        CanyonRun::from_records(records).ok_or(Error::Damaged)?;
        let mut coordinates = [(0, 0); 5];
        for (slot, record) in records.iter().enumerate() {
            coordinates[slot] = record.region_id;
        }
        Ok(Self {
            count: records.len() as u8,
            coordinates,
        })
    }
    pub fn keys(&self) -> Result<&[(i32, i32)], Error> {
        if !(1..=5).contains(&self.count) {
            return Err(Error::Damaged);
        }
        let keys = &self.coordinates[..usize::from(self.count)];
        if keys
            .iter()
            .enumerate()
            .any(|(i, key)| keys[..i].contains(key))
        {
            return Err(Error::Damaged);
        }
        Ok(keys)
    }
}
pub fn decode_canyon(bytes: &[u8], key: (i32, i32)) -> Result<CanyonRegionRecord, Error> {
    if bytes.len() > CANYON_RECORD_LIMIT {
        return Err(Error::TooLarge);
    }
    let record: CanyonRegionRecord = postcard::from_bytes(bytes).map_err(|_| Error::Damaged)?;
    if record.region_id != key || !record.valid() {
        return Err(Error::Damaged);
    }
    Ok(record)
}
impl Capture {
    pub fn with_canyon(mut self, records: Vec<CanyonRegionRecord>) -> Result<Self, Error> {
        CanyonIndex::new(&records)?;
        for record in &records {
            if postcard::to_allocvec(record)
                .map_err(|_| Error::Damaged)?
                .len()
                > CANYON_RECORD_LIMIT
            {
                return Err(Error::TooLarge);
            }
        }
        self.canyon = Some(records);
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canyon_drift::run::SlotPhase;
    use bevy_math::Vec2;

    #[test]
    fn ten_thousand_generations_keep_a_fixed_page_and_encoded_bundle_bound() {
        let mut run = CanyonRun::new(71);
        let mut peak = 0;
        for generation in 1..=10_000 {
            for (i, slot) in run.slots.iter_mut().enumerate() {
                slot.generation = generation;
                slot.revision = u64::from(generation) * 4;
                slot.location =
                    Vec2::new(1000.0 + generation as f32 * 64.0 + i as f32 * 512.0, 40.0);
                slot.phase = SlotPhase::WashedOut;
            }
            run.catcher.position = Vec2::new(800_000.0 - generation as f32 * 8.0, 100.0);
            run.catcher.revision = u64::from(generation);
            run.practice.position = Vec2::new(600_000.0 - generation as f32 * 8.0, 50.0);
            run.practice_revision = u64::from(generation);
            let records = run.records();
            let index = CanyonIndex::new(&records).unwrap();
            assert!(index.keys().unwrap().len() <= 5);
            let bytes = postcard::to_allocvec(&index).unwrap();
            assert!(bytes.len() <= CANYON_INDEX_LIMIT);
            let mut total = bytes.len();
            for record in &records {
                let bytes = postcard::to_allocvec(record).unwrap();
                assert!(bytes.len() <= CANYON_RECORD_LIMIT);
                assert_eq!(decode_canyon(&bytes, record.region_id).unwrap(), *record);
                total += bytes.len();
            }
            peak = peak.max(total);
            assert_eq!(CanyonRun::from_records(&records), Some(run.clone()));
        }
        println!("10,000 generations: <=5 Canyon pages, peak encoded bundle {peak} bytes");
        assert!(peak < 2048);
    }

    #[test]
    fn malformed_and_incomplete_indexes_or_pages_are_rejected() {
        let records = CanyonRun::new(41).records();
        assert!(CanyonIndex::new(&[]).is_err());
        let mut duplicate = records.clone();
        duplicate.push(records[0].clone());
        assert!(CanyonIndex::new(&duplicate).is_err());
        assert!(
            CanyonIndex {
                count: 6,
                coordinates: [(0, 0); 5]
            }
            .keys()
            .is_err()
        );
        assert!(
            CanyonIndex {
                count: 2,
                coordinates: [(1, 2); 5]
            }
            .keys()
            .is_err()
        );
        let record = &records[0];
        let raw = postcard::to_allocvec(record).unwrap();
        assert!(decode_canyon(&raw, (record.region_id.0 + 1, record.region_id.1)).is_err());
        assert!(decode_canyon(&vec![0; CANYON_RECORD_LIMIT + 1], record.region_id).is_err());
        assert!(CanyonRun::from_records(&records[..records.len() - 1]).is_none());
    }
}
