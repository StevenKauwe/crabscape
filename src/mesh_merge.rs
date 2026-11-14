//! Still shapes drawn as a few big meshes instead of thousands of small ones.
//!
//! On WebGL2 (every iPhone browser) Bevy gives each mesh its own GPU buffer, because WebGL can't start a draw part
//! way into a shared one. The world is built from about ten thousand small shapes (rocks, weed, grains, ridges...),
//! so a phone held about 18,000 buffers at once; Safari charges each to the page, and the page was killed as soon as
//! the crab moved (and on a desktop, ten thousand draws a frame made it slow).
//!
//! So each still shape with a shared material is folded, the frame it appears, into one mesh per patch of the map,
//! material and depth: its vertices are moved to where it stands and its own mesh is dropped. A shape that later
//! moves, changes, hides or goes gets its own mesh back and its patch is rebuilt without it; it is never folded
//! again. A shape that must keep its own mesh (one whose mesh is rewritten in place) carries [`NoMerge`].
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::VisibilitySystems;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::sprite_render::{Material2d, MeshMaterial2d};
use bevy::transform::TransformSystems;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

/// The size of a patch of the map whose still shapes share one mesh (world units).
pub const PATCH: f32 = 1024.0;

/// Keep this shape's own mesh (its mesh is changed in place, or it is drawn on its own).
#[derive(Component, Default, Clone, Copy)]
pub struct NoMerge;

/// A shape folded into a patch mesh.
#[derive(Component)]
pub struct Folded;

/// A shape that changed after it appeared: it keeps its own mesh from now on.
#[derive(Component)]
pub struct Restless;

/// A patch mesh's entity.
#[derive(Component)]
pub struct Patch;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct PatchKey {
    material: u64,
    z: u32,
    x: i32,
    y: i32,
    layout: u64,
}

/// One folded shape: its vertices where it stands, and its own mesh to give back.
struct Piece {
    key: PatchKey,
    own: Mesh,
    placed: Mesh,
}

#[derive(Default)]
struct PatchState {
    pieces: HashSet<Entity>,
    entity: Option<Entity>,
    dirty: bool,
}

#[derive(Resource)]
struct Folds<M: Material2d> {
    pieces: HashMap<Entity, Piece>,
    patches: HashMap<PatchKey, PatchState>,
    _m: PhantomData<M>,
}

impl<M: Material2d> Default for Folds<M> {
    fn default() -> Self {
        Self {
            pieces: HashMap::new(),
            patches: HashMap::new(),
            _m: PhantomData,
        }
    }
}

/// Folds still shapes drawn with material `M`.
pub struct MeshMergePlugin<M: Material2d>(PhantomData<M>);

impl<M: Material2d> Default for MeshMergePlugin<M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<M: Material2d> Plugin for MeshMergePlugin<M> {
    fn build(&self, app: &mut App) {
        app.init_resource::<Folds<M>>().add_systems(
            PostUpdate,
            (share_meshes, unfold::<M>, fold::<M>, rebuild::<M>)
                .chain()
                .after(TransformSystems::Propagate)
                .after(VisibilitySystems::VisibilityPropagate),
        );
    }
}

/// A stable id for a mesh's vertex layout (which attributes, in which formats).
fn layout_of(mesh: &Mesh) -> Option<u64> {
    let mut ids: Vec<(String, String)> = mesh
        .try_attributes()
        .ok()?
        .map(|(a, v)| {
            (
                a.name.to_string(),
                format!("{:?}", bevy::mesh::VertexFormat::from(v)),
            )
        })
        .collect();
    ids.sort();
    Some(hash_of(&ids))
}

/// The mesh moved to where its shape stands (positions through `at`; everything else as it was).
fn placed(mesh: &Mesh, at: &GlobalTransform, z: f32) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let affine = at.affine();
    let mut out = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut count = 0;
    for (attribute, values) in mesh.try_attributes().ok()? {
        let values = if attribute.id == Mesh::ATTRIBUTE_POSITION.id {
            let VertexAttributeValues::Float32x3(p) = values else {
                return None;
            };
            count = p.len();
            VertexAttributeValues::Float32x3(
                p.iter()
                    .map(|v| {
                        let w = affine.transform_point3(Vec3::from_array(*v));
                        [w.x, w.y, w.z - z]
                    })
                    .collect(),
            )
        } else {
            values.clone()
        };
        out.insert_attribute(*attribute, values);
    }
    if count == 0 {
        return None;
    }
    let indices: Vec<u32> = match mesh.indices() {
        Some(Indices::U16(i)) => i.iter().map(|&i| i as u32).collect(),
        Some(Indices::U32(i)) => i.clone(),
        None => (0..count as u32).collect(),
    };
    out.insert_indices(Indices::U32(indices));
    Some(out)
}

/// Appends `b`'s values to `a` (same format; false if they differ or the format isn't handled).
fn append(a: &mut VertexAttributeValues, b: &VertexAttributeValues) -> bool {
    use VertexAttributeValues as V;
    match (a, b) {
        (V::Float32(a), V::Float32(b)) => a.extend_from_slice(b),
        (V::Float32x2(a), V::Float32x2(b)) => a.extend_from_slice(b),
        (V::Float32x3(a), V::Float32x3(b)) => a.extend_from_slice(b),
        (V::Float32x4(a), V::Float32x4(b)) => a.extend_from_slice(b),
        (V::Uint32(a), V::Uint32(b)) => a.extend_from_slice(b),
        (V::Unorm8x4(a), V::Unorm8x4(b)) => a.extend_from_slice(b),
        _ => return false,
    }
    true
}

/// Meshes already drawn, by content: a new shape whose mesh is the same as one of these draws that one.
#[derive(Resource, Default)]
struct SameMeshes(HashMap<u64, AssetId<Mesh>>);

fn mesh_hash(mesh: &Mesh) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", mesh.primitive_topology()).hash(&mut h);
    let mut parts: Vec<(String, &[u8])> = mesh
        .try_attributes()
        .ok()?
        .map(|(a, v)| (a.name.to_string(), v.get_bytes()))
        .collect();
    parts.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, bytes) in parts {
        name.hash(&mut h);
        bytes.hash(&mut h);
    }
    match mesh.indices() {
        Some(Indices::U16(i)) => i.hash(&mut h),
        Some(Indices::U32(i)) => i.hash(&mut h),
        None => 0u8.hash(&mut h),
    }
    Some(h.finish())
}

/// A new shape whose mesh matches one already made draws that one (each mesh is its own GPU buffer on WebGL2).
#[allow(clippy::type_complexity)]
fn share_meshes(
    mut same: Local<SameMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut new: Query<
        &mut Mesh2d,
        (
            Added<Mesh2d>,
            Without<NoMerge>,
            Without<Sprite>,
            Without<Patch>,
        ),
    >,
) {
    for mut mesh in &mut new {
        let Some(own) = meshes.get(&mesh.0) else {
            continue;
        };
        let Some(hash) = mesh_hash(own) else {
            continue;
        };
        match same
            .0
            .get(&hash)
            .and_then(|id| meshes.get_strong_handle(*id))
        {
            Some(handle) if handle.id() != mesh.0.id() => mesh.0 = handle,
            Some(_) => {}
            None => {
                same.0.insert(hash, mesh.0.id());
            }
        }
    }
    if same.0.len() > 4 * meshes.len() + 1024 {
        same.0.retain(|_, id| meshes.contains(*id));
    }
}

/// Shapes that moved, changed, hid or went: their own mesh back, their patch rebuilt.
#[allow(clippy::type_complexity)]
fn unfold<M: Material2d>(
    mut commands: Commands,
    mut folds: ResMut<Folds<M>>,
    changed: Query<
        Entity,
        (
            With<Folded>,
            Or<(
                Changed<GlobalTransform>,
                Changed<MeshMaterial2d<M>>,
                Changed<InheritedVisibility>,
                Added<Mesh2d>,
            )>,
        ),
    >,
    mut gone: RemovedComponents<Folded>,
) {
    let folds = &mut *folds;
    for entity in gone.read() {
        if let Some(piece) = folds.pieces.remove(&entity)
            && let Some(patch) = folds.patches.get_mut(&piece.key)
        {
            patch.pieces.remove(&entity);
            patch.dirty = true;
        }
    }
    for entity in &changed {
        let Some(piece) = folds.pieces.remove(&entity) else {
            continue;
        };
        if let Some(patch) = folds.patches.get_mut(&piece.key) {
            patch.pieces.remove(&entity);
            patch.dirty = true;
        }
        let mut e = commands.entity(entity);
        e.remove::<Folded>().insert(Restless);
        // A shape that was given a new mesh keeps it; otherwise it gets its own back.
        e.queue(move |mut entity: EntityWorldMut| {
            if !entity.contains::<Mesh2d>() {
                let handle =
                    entity.world_scope(|w| w.resource_mut::<Assets<Mesh>>().add(piece.own));
                entity.insert(Mesh2d(handle));
            }
        });
    }
}

/// Shapes that just appeared, still and shown: folded into their patch, their own mesh dropped.
#[allow(clippy::type_complexity)]
fn fold<M: Material2d>(
    mut commands: Commands,
    mut folds: ResMut<Folds<M>>,
    meshes: Res<Assets<Mesh>>,
    new: Query<
        (
            Entity,
            &Mesh2d,
            &MeshMaterial2d<M>,
            &GlobalTransform,
            &InheritedVisibility,
        ),
        (
            Added<Mesh2d>,
            Without<NoMerge>,
            Without<Restless>,
            Without<Folded>,
            Without<Patch>,
            Without<Sprite>,
        ),
    >,
) {
    let folds = &mut *folds;
    for (entity, mesh, material, at, shown) in &new {
        if !shown.get() {
            continue;
        }
        let Some(own) = meshes.get(&mesh.0) else {
            continue;
        };
        if own.try_attributes().is_err() {
            continue;
        }
        let Some(layout) = layout_of(own) else {
            continue;
        };
        let z = at.translation().z;
        let Some(placed) = placed(own, at, z) else {
            continue;
        };
        let centre = at.translation();
        let key = PatchKey {
            material: hash_of(&material.0.id()),
            z: z.to_bits(),
            x: (centre.x / PATCH).floor() as i32,
            y: (centre.y / PATCH).floor() as i32,
            layout,
        };
        let own = own.clone();
        let patch = folds.patches.entry(key).or_default();
        patch.pieces.insert(entity);
        patch.dirty = true;
        folds.pieces.insert(entity, Piece { key, own, placed });
        commands.entity(entity).remove::<Mesh2d>().insert(Folded);
    }
}

fn hash_of<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

/// Patches whose shapes changed get a fresh mesh (or go, when none are left).
fn rebuild<M: Material2d>(
    mut commands: Commands,
    mut folds: ResMut<Folds<M>>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Query<&MeshMaterial2d<M>>,
) {
    let folds = &mut *folds;
    for (key, patch) in folds.patches.iter_mut() {
        if !patch.dirty {
            continue;
        }
        patch.dirty = false;
        let mut merged: Option<Mesh> = None;
        let mut material = None;
        for entity in &patch.pieces {
            let Some(piece) = folds.pieces.get(entity) else {
                continue;
            };
            if material.is_none() {
                material = materials.get(*entity).ok().map(|m| m.0.clone());
            }
            merged = Some(match merged {
                None => piece.placed.clone(),
                Some(mut m) => {
                    let base = m.count_vertices() as u32;
                    for (attribute, values) in piece.placed.attributes() {
                        if let Some(into) = m.attribute_mut(attribute.id) {
                            append(into, values);
                        }
                    }
                    if let (Some(Indices::U32(into)), Some(Indices::U32(from))) =
                        (m.indices_mut(), piece.placed.indices())
                    {
                        into.extend(from.iter().map(|i| i + base));
                    }
                    m
                }
            });
        }
        if let Some(entity) = patch.entity.take() {
            commands.entity(entity).despawn();
        }
        let (Some(merged), Some(material)) = (merged, material) else {
            continue;
        };
        let aabb = match merged.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) if !p.is_empty() => {
                let (lo, hi) = p.iter().fold(
                    (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                    |(lo, hi), v| (lo.min(Vec3::from_array(*v)), hi.max(Vec3::from_array(*v))),
                );
                Some(Aabb::from_min_max(lo, hi))
            }
            _ => None,
        };
        let mut e = commands.spawn((
            Patch,
            Mesh2d(meshes.add(merged)),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, f32::from_bits(key.z)),
        ));
        if let Some(aabb) = aabb {
            e.insert(aabb);
        }
        patch.entity = Some(e.id());
    }
    folds
        .patches
        .retain(|_, p| !p.pieces.is_empty() || p.entity.is_some());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placed_mesh_sits_where_its_shape_stands() {
        let mesh = Mesh::from(Rectangle::new(2.0, 2.0));
        let at = GlobalTransform::from(Transform::from_xyz(10.0, 20.0, 3.0));
        let p = placed(&mesh, &at, 3.0).unwrap();
        let Some(VertexAttributeValues::Float32x3(v)) = p.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        assert!(
            v.iter()
                .all(|v| (v[0] - 10.0).abs() <= 1.0 && (v[1] - 20.0).abs() <= 1.0 && v[2] == 0.0)
        );
    }
}
