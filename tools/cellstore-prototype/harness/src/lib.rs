use crabsim::underwater::{
    World,
    frozen_chunks::{FrozenChunks, FrozenRestore},
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Proof {
    world: World,
    frozen: FrozenChunks,
    sleeping: Vec<(i32, i32)>,
    restore: Option<FrozenRestore>,
}

fn js(error: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{error:?}"))
}

#[wasm_bindgen]
impl Proof {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Proof, JsValue> {
        let level = crabsim::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.follow_player(&level, 4000.0, 0.0);
        world.follow_player(&level, 8000.0, 0.0);
        let frozen = FrozenChunks::capture(&world, 64 * 1024 * 1024).map_err(js)?;
        let sleeping = frozen.sleeping_ids().collect();
        Ok(Self {
            world,
            frozen,
            sleeping,
            restore: None,
        })
    }
    pub fn tick(&self) -> f64 {
        self.frozen.tick() as f64
    }
    pub fn estimate(&self) -> usize {
        self.frozen.estimated_bytes()
    }
    pub fn cols(&self) -> usize {
        self.frozen.active_dimensions().0
    }
    pub fn rows(&self) -> usize {
        self.frozen.active_dimensions().1
    }
    pub fn sleeping_count(&self) -> usize {
        self.sleeping.len()
    }
    pub fn metadata(&self) -> Result<Vec<u8>, JsValue> {
        self.frozen.metadata().map_err(js)
    }
    pub fn active(&self, x: usize, y: usize) -> Result<Vec<u8>, JsValue> {
        self.frozen.active(x, y).map_err(js)
    }
    pub fn sleeping(&self, index: usize) -> Result<Vec<u8>, JsValue> {
        self.frozen
            .sleeping(*self.sleeping.get(index).ok_or_else(|| js("index"))?)
            .map_err(js)
    }
    pub fn begin_restore(&mut self, metadata: &[u8], count: usize) -> Result<(), JsValue> {
        self.restore = Some(FrozenRestore::new(metadata, count).map_err(js)?);
        Ok(())
    }
    pub fn restore_active(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.restore
            .as_mut()
            .ok_or_else(|| js("not started"))?
            .active(bytes)
            .map_err(js)
    }
    pub fn restore_sleeping(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.restore
            .as_mut()
            .ok_or_else(|| js("not started"))?
            .sleeping(bytes)
            .map_err(js)
    }
    pub fn finish_restore(&mut self) -> Result<bool, JsValue> {
        let restored = self
            .restore
            .take()
            .ok_or_else(|| js("not started"))?
            .finish()
            .map_err(js)?;
        Ok(restored.checkpoint() == self.world.checkpoint())
    }
}
