//! Reusable drawing-order scratch. It is presentation-only, shared sequentially
//! by the animation system, and grows only to the largest loaded puppet rig.

#[derive(Default)]
pub(crate) struct DepthOrder {
    keys: Vec<f32>,
    indices: Vec<usize>,
    slots: Vec<i32>,
}

impl DepthOrder {
    pub(crate) fn slots(&mut self, keys: impl Iterator<Item = f32>) -> &[i32] {
        self.keys.clear();
        self.keys.extend(keys);
        self.indices.clear();
        self.indices.extend(0..self.keys.len());
        // Explicit original-index ties preserve the old stable sort's exact
        // order, while the unstable sort needs no auxiliary allocation.
        self.indices
            .sort_unstable_by(|&a, &b| self.keys[a].total_cmp(&self.keys[b]).then(a.cmp(&b)));
        let behind = self.keys.iter().filter(|&&key| key < 0.0).count() as i32;
        self.slots.resize(self.keys.len(), 0);
        for (rank, &index) in self.indices.iter().enumerate() {
            let rank = rank as i32 - behind;
            self.slots[index] = if rank >= 0 { rank + 1 } else { rank };
        }
        &self.slots
    }
}

/// Compare the entire previous signature before updating it in place. Root
/// presence and its exact float bits matter, just as in the old Vec signature.
pub(crate) fn remember_draw_order(
    previous: &mut Vec<i32>,
    slots: &[i32],
    lantern_in_front: bool,
    root_z: Option<f32>,
) -> bool {
    let signature = slots
        .iter()
        .copied()
        .chain(std::iter::once(i32::from(lantern_in_front)))
        .chain(root_z.map(|z| z.to_bits() as i32));
    if previous.iter().copied().eq(signature.clone()) {
        return false;
    }
    previous.clear();
    previous.extend(signature);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original_slots(keys: &[f32]) -> Vec<i32> {
        let mut indices: Vec<_> = (0..keys.len()).collect();
        indices.sort_by(|&a, &b| keys[a].total_cmp(&keys[b]));
        let behind = keys.iter().filter(|&&key| key < 0.0).count() as i32;
        let mut slots = vec![0; keys.len()];
        for (rank, &index) in indices.iter().enumerate() {
            let rank = rank as i32 - behind;
            slots[index] = if rank >= 0 { rank + 1 } else { rank };
        }
        slots
    }

    #[test]
    fn ordering_matches_stable_reference_including_ties_and_float_edges() {
        let mut scratch = DepthOrder::default();
        let edges = [
            f32::NEG_INFINITY,
            -1.0,
            -0.0,
            0.0,
            1.0,
            f32::INFINITY,
            f32::NAN,
        ];
        for size in [0, 1, 2, 12, 16, 64] {
            for rotation in 0..edges.len() {
                let keys: Vec<_> = (0..size)
                    .map(|i| edges[(i + rotation) % edges.len()])
                    .collect();
                assert_eq!(scratch.slots(keys.iter().copied()), original_slots(&keys));
            }
        }
    }

    #[test]
    fn scratch_reuses_buffers_across_frames_and_differently_sized_puppets() {
        let mut scratch = DepthOrder::default();
        scratch.slots((0..16).map(|i| i as f32));
        let buffers = (
            (scratch.keys.as_ptr(), scratch.keys.capacity()),
            (scratch.indices.as_ptr(), scratch.indices.capacity()),
            (scratch.slots.as_ptr(), scratch.slots.capacity()),
        );
        for frame in 0..1200 {
            let count = [12, 0, 16, 8][frame % 4];
            scratch.slots((0..count).map(|i| ((i + frame) % 7) as f32 - 3.0));
            assert_eq!(
                buffers,
                (
                    (scratch.keys.as_ptr(), scratch.keys.capacity()),
                    (scratch.indices.as_ptr(), scratch.indices.capacity()),
                    (scratch.slots.as_ptr(), scratch.slots.capacity()),
                )
            );
        }
    }

    #[test]
    fn resort_signature_preserves_change_detection_and_reuses_capacity() {
        let mut old = Vec::new();
        let mut new = Vec::new();
        // Include repeated frames, reorderings, rig removal/rebuild, lantern
        // flips, missing roots and signed zero in root depth.
        for (slots, lantern, root) in [
            (&[-1, 1, 2][..], false, Some(0.0)),
            (&[-1, 1, 2][..], false, Some(0.0)),
            (&[1, -1, 2][..], false, Some(0.0)),
            (&[1, -1, 2][..], true, Some(0.0)),
            (&[1, -1, 2][..], true, Some(-0.0)),
            (&[1, -1, 2][..], true, None),
            (&[][..], true, None),
            (&[-1, 1, 2][..], false, Some(9.0)),
        ] {
            let mut wanted = slots.to_vec();
            wanted.push(i32::from(lantern));
            if let Some(z) = root {
                wanted.push(f32::to_bits(z) as i32);
            }
            assert_eq!(
                remember_draw_order(&mut new, slots, lantern, root),
                old != wanted
            );
            assert_eq!(new, wanted);
            old = wanted;
        }
        let buffer = (new.as_ptr(), new.capacity());
        for frame in 0..1200 {
            remember_draw_order(&mut new, &[-1, 1, 2], frame % 2 == 0, Some(9.0));
            assert_eq!(buffer, (new.as_ptr(), new.capacity()));
        }
    }
}
