//! Bounded framing shared by the browser caller and compression worker.
//! Records remain individually compressed and verified; this is not a save format.
pub const MAX_RECORDS: usize = 16;
pub const MAX_RAW_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub fn pack(records: &[&[u8]]) -> Result<Vec<u8>, &'static str> {
    if records.is_empty() || records.len() > MAX_RECORDS {
        return Err("invalid compression batch count");
    }
    let length = records
        .iter()
        .try_fold(4usize, |n, record| {
            n.checked_add(4)?.checked_add(record.len())
        })
        .ok_or("compression batch overflow")?;
    if length > MAX_FRAME_BYTES {
        return Err("compression batch too large");
    }
    let mut frame = Vec::with_capacity(length);
    frame.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for record in records {
        frame.extend_from_slice(&(record.len() as u32).to_le_bytes());
        frame.extend_from_slice(record);
    }
    Ok(frame)
}
pub fn unpack(frame: &[u8]) -> Result<Vec<&[u8]>, &'static str> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err("compression batch too large");
    }
    fn number(input: &mut &[u8]) -> Result<usize, &'static str> {
        let (number, rest) = input
            .split_at_checked(4)
            .ok_or("truncated compression batch")?;
        *input = rest;
        Ok(u32::from_le_bytes(number.try_into().map_err(|_| "batch length")?) as usize)
    }
    let mut input = frame;
    let count = number(&mut input)?;
    if count == 0 || count > MAX_RECORDS {
        return Err("invalid compression batch count");
    }
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let length = number(&mut input)?;
        let (record, rest) = input
            .split_at_checked(length)
            .ok_or("truncated compression record")?;
        records.push(record);
        input = rest;
    }
    if !input.is_empty() {
        return Err("trailing compression batch bytes");
    }
    Ok(records)
}
pub fn raw_records(frame: &[u8]) -> Result<Vec<&[u8]>, &'static str> {
    let records = unpack(frame)?;
    if records.iter().map(|r| r.len()).sum::<usize>() > MAX_RAW_BYTES {
        return Err("raw compression batch too large");
    }
    Ok(records)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_preserves_order_empty_records_and_boundaries() {
        let records: &[&[u8]] = &[b"", b"abc", &[0, 1, 255]];
        assert_eq!(unpack(&pack(records).unwrap()).unwrap(), records);
        let maximum = vec![0; MAX_RAW_BYTES];
        assert_eq!(
            raw_records(&pack(&[&maximum]).unwrap()).unwrap()[0],
            maximum
        );
    }
    #[test]
    fn malformed_and_oversized_frames_are_rejected() {
        let frame = pack(&[b"abc", b"def"]).unwrap();
        for end in 0..frame.len() {
            assert!(unpack(&frame[..end]).is_err());
        }
        let mut extra = frame.clone();
        extra.push(0);
        assert!(unpack(&extra).is_err());
        assert!(pack(&[]).is_err());
        assert!(pack(&[b"".as_slice(); MAX_RECORDS + 1]).is_err());
        assert!(raw_records(&pack(&[&vec![0; MAX_RAW_BYTES + 1]]).unwrap()).is_err());
        let mut bad = frame;
        bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(unpack(&bad).is_err());
    }
}
