//! Read-only projection of the local CrabScape puppet into the travel scene.
//!
//! There is no second crab, inventory, animation rig or simulation here. The
//! existing puppet supplies its current transforms, visibility, atlases and
//! carried meshes. Only static mesh topology is cached; pose and appearance are
//! read again each frame. Nothing above the puppet root (including its world
//! position and camera visibility) is copied or changed. While travelling,
//! render layers temporarily hide the source pictures so only one crab is seen;
//! their exact previous layers are restored when the visit ends.
use std::{collections::HashMap, sync::Arc};

use bevy::{
    camera::visibility::{RenderLayers, VisibilitySystems},
    ecs::query::QueryData,
    math::{Affine2, Affine3A},
    mesh::{PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    sprite::Anchor,
};
use bevy_egui::{EguiPostUpdateSet, EguiTextureHandle, EguiUserTextures, egui};

/// Attached only to the existing local crab's puppet root by `puppet.rs`.
#[derive(Component)]
pub(crate) struct CrabAvatarSource;

// Independent caps protect against accidentally attaching a world subtree or
// an oversized cosmetic. Normal crab puppets use far less than these budgets.
const MAX_NODES: usize = 768;
const MAX_PARTS: usize = 384;
const MAX_VERTICES: usize = 16_384;
const MAX_INDICES: usize = 49_152;
const MAX_CACHED_MESHES: usize = 64;
const MAX_MESH_VERTICES: usize = 2_048;

#[derive(Clone, Copy)]
struct Vertex {
    position: Vec3,
    uv: Vec2,
    color: LinearRgba,
}

struct Geometry {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    bounds: Rect,
}

enum Shape {
    Quad([Vertex; 4]),
    Mesh(Arc<Geometry>),
}
impl Shape {
    fn vertices(&self) -> &[Vertex] {
        match self {
            Self::Quad(vertices) => vertices,
            Self::Mesh(mesh) => &mesh.vertices,
        }
    }
    fn indices(&self) -> &[u32] {
        match self {
            Self::Quad(_) => &[0, 1, 2, 0, 2, 3],
            Self::Mesh(mesh) => &mesh.indices,
        }
    }
    fn bounds(&self) -> Rect {
        match self {
            Self::Quad(vertices) => Rect::from_corners(
                vertices[0].position.truncate(),
                vertices[2].position.truncate(),
            ),
            Self::Mesh(mesh) => mesh.bounds,
        }
    }
}

struct Part {
    shape: Shape,
    transform: Affine3A,
    texture: egui::TextureId,
    tint: LinearRgba,
    uv_transform: Affine2,
}

struct SourceLayers {
    previous: Option<RenderLayers>,
    seen: bool,
}

/// Derived visual data only. Screen placement belongs to the travel scene.
#[derive(Resource, Default)]
pub(crate) struct CrabAvatar {
    parts: Vec<Part>,
    order: Vec<usize>,
    stack: Vec<(Entity, Affine3A, bool)>,
    meshes: HashMap<AssetId<Mesh>, Arc<Geometry>>,
    source_layers: HashMap<Entity, SourceLayers>,
    bounds: Option<Rect>,
    vertices: usize,
    indices: usize,
}

impl CrabAvatar {
    pub(crate) fn is_ready(&self) -> bool {
        self.bounds.is_some_and(|bounds| bounds.width() > 0.) && !self.parts.is_empty()
    }

    /// Draw at the supplied feet position, with the whole puppet's current
    /// visual width in screen points. Growth, pose and cosmetics retain their
    /// proportions. The painter's clip rectangle is respected.
    pub(crate) fn paint(&self, painter: &egui::Painter, feet: egui::Pos2, width_pixels: f32) {
        let Some(bounds) = self.bounds else { return };
        if !feet.is_finite()
            || !width_pixels.is_finite()
            || width_pixels <= 0.
            || bounds.width() <= 0.
        {
            return;
        }
        let scale = width_pixels.min(4096.) / bounds.width();
        if !scale.is_finite() {
            return;
        }
        let projected = egui::Rect::from_two_pos(
            feet + egui::vec2(bounds.min.x, -bounds.min.y) * scale,
            feet + egui::vec2(bounds.max.x, -bounds.max.y) * scale,
        );
        if !projected.intersects(painter.clip_rect()) {
            return;
        }
        // Adjacent pieces sharing a texture are one draw, without reordering
        // translucent layers. Egui owns only the resulting screen vertices.
        let mut mesh = egui::Mesh::default();
        for &index in &self.order {
            let part = &self.parts[index];
            if mesh.texture_id != part.texture && !mesh.is_empty() {
                painter.add(std::mem::take(&mut mesh));
            }
            mesh.texture_id = part.texture;
            let first = mesh.vertices.len() as u32;
            mesh.indices
                .extend(part.shape.indices().iter().map(|i| first + i));
            mesh.vertices
                .extend(part.shape.vertices().iter().map(|vertex| {
                    let at = part.transform.transform_point3(vertex.position);
                    let uv = if part.texture == egui::TextureId::default() {
                        egui::epaint::WHITE_UV
                    } else {
                        let uv = part.uv_transform.transform_point2(vertex.uv);
                        egui::pos2(uv.x, uv.y)
                    };
                    egui::epaint::Vertex {
                        pos: feet + egui::vec2(at.x, -at.y) * scale,
                        uv,
                        color: color32(LinearRgba::new(
                            vertex.color.red * part.tint.red,
                            vertex.color.green * part.tint.green,
                            vertex.color.blue * part.tint.blue,
                            vertex.color.alpha * part.tint.alpha,
                        )),
                    }
                }));
        }
        if !mesh.is_empty() {
            painter.add(mesh);
        }
    }

    fn clear_pose(&mut self) {
        self.parts.clear();
        self.order.clear();
        self.stack.clear();
        self.bounds = None;
        self.vertices = 0;
        self.indices = 0;
    }

    fn push(&mut self, part: Part) {
        let vertices = part.shape.vertices().len();
        let indices = part.shape.indices().len();
        if self.parts.len() >= MAX_PARTS
            || self.vertices + vertices > MAX_VERTICES
            || self.indices + indices > MAX_INDICES
        {
            return;
        }
        let bounds = part.shape.bounds();
        for point in [
            bounds.min,
            bounds.max,
            Vec2::new(bounds.min.x, bounds.max.y),
            Vec2::new(bounds.max.x, bounds.min.y),
        ] {
            let at = part.transform.transform_point3(point.extend(0.)).truncate();
            if !at.is_finite() || at.abs().max_element() > 1_000_000. {
                return;
            }
        }
        for point in [
            bounds.min,
            bounds.max,
            Vec2::new(bounds.min.x, bounds.max.y),
            Vec2::new(bounds.max.x, bounds.min.y),
        ] {
            let at = part.transform.transform_point3(point.extend(0.)).truncate();
            self.bounds = Some(self.bounds.map_or(Rect::from_corners(at, at), |bounds| {
                Rect::from_corners(bounds.min.min(at), bounds.max.max(at))
            }));
        }
        self.order.push(self.parts.len());
        self.parts.push(part);
        self.vertices += vertices;
        self.indices += indices;
    }

    fn geometry(&mut self, id: AssetId<Mesh>, meshes: &Assets<Mesh>) -> Option<Arc<Geometry>> {
        if let Some(geometry) = self.meshes.get(&id) {
            return Some(Arc::clone(geometry));
        }
        if self.meshes.len() >= MAX_CACHED_MESHES {
            return None;
        }
        let geometry = source_geometry(meshes.get(id)?)?;
        let cached_vertices: usize = self.meshes.values().map(|mesh| mesh.vertices.len()).sum();
        let cached_indices: usize = self.meshes.values().map(|mesh| mesh.indices.len()).sum();
        if cached_vertices + geometry.vertices.len() > MAX_VERTICES
            || cached_indices + geometry.indices.len() > MAX_INDICES
        {
            return None;
        }
        let geometry = Arc::new(geometry);
        self.meshes.insert(id, Arc::clone(&geometry));
        Some(geometry)
    }

    fn suppress_source(
        &mut self,
        entity: Entity,
        layers: Option<&RenderLayers>,
        commands: &mut Commands,
    ) {
        if self.source_layers.len() >= MAX_NODES && !self.source_layers.contains_key(&entity) {
            return;
        }
        self.source_layers
            .entry(entity)
            .or_insert_with(|| SourceLayers {
                previous: layers.cloned(),
                seen: false,
            })
            .seen = true;
        if layers.is_none_or(|layers| *layers != RenderLayers::none()) {
            commands.entity(entity).insert(RenderLayers::none());
        }
    }

    fn restore_unseen(&mut self, commands: &mut Commands) {
        self.source_layers.retain(|entity, layers| {
            if layers.seen {
                return true;
            }
            if let Ok(mut entity) = commands.get_entity(*entity) {
                if let Some(previous) = &layers.previous {
                    entity.insert(previous.clone());
                } else {
                    entity.remove::<RenderLayers>();
                }
            }
            false
        });
    }
}

pub(crate) struct CrabAvatarPlugin;
impl Plugin for CrabAvatarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrabAvatar>().add_systems(
            PostUpdate,
            capture
                .before(EguiPostUpdateSet::EndPass)
                .before(VisibilitySystems::CheckVisibility),
        );
    }
}

#[derive(QueryData)]
struct VisualNode {
    transform: &'static Transform,
    visibility: Option<&'static Visibility>,
    children: Option<&'static Children>,
    sprite: Option<&'static Sprite>,
    anchor: Option<&'static Anchor>,
    mesh: Option<&'static Mesh2d>,
    material: Option<&'static MeshMaterial2d<ColorMaterial>>,
    layers: Option<&'static RenderLayers>,
}

#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    state: Res<super::Valmora>,
    roots: Query<Entity, With<CrabAvatarSource>>,
    nodes: Query<VisualNode>,
    images: Res<Assets<Image>>,
    atlases: Res<Assets<TextureAtlasLayout>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<ColorMaterial>>,
    mut changed: MessageReader<AssetEvent<Mesh>>,
    mut textures: ResMut<EguiUserTextures>,
    mut avatar: ResMut<CrabAvatar>,
) {
    for event in changed.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            avatar.meshes.remove(id);
        }
    }
    avatar.clear_pose();
    for layers in avatar.source_layers.values_mut() {
        layers.seen = false;
    }
    if !state.active && !state.travelling {
        // A new trip may wear different equipment. Do not accumulate previous
        // trips' cosmetic assets, and do no hierarchy work while at home.
        avatar.meshes.clear();
        avatar.restore_unseen(&mut commands);
        return;
    }
    let Ok(root) = roots.single() else {
        avatar.meshes.clear();
        avatar.restore_unseen(&mut commands);
        return;
    };
    avatar.stack.push((root, Affine3A::IDENTITY, true));
    let mut visited = 0;
    while visited < MAX_NODES {
        let Some((entity, parent, inherited)) = avatar.stack.pop() else {
            break;
        };
        visited += 1;
        let Ok(node) = nodes.get(entity) else {
            continue;
        };
        if node.sprite.is_some() || node.mesh.is_some() {
            avatar.suppress_source(entity, node.layers, &mut commands);
        }
        let visible = match node.visibility {
            Some(Visibility::Hidden) => false,
            Some(Visibility::Visible) => true,
            _ => inherited,
        };
        let transform = parent * node.transform.compute_affine();
        if !transform.matrix3.is_finite() || !transform.translation.is_finite() {
            continue;
        }
        // Hidden parents still need traversal: Bevy permits an explicitly
        // Visible child. Only inherited visibility follows the parent.
        if let Some(children) = node.children {
            let remaining = MAX_NODES.saturating_sub(visited + avatar.stack.len());
            avatar.stack.extend(
                children
                    .iter()
                    .take(remaining)
                    .rev()
                    .map(|child| (child, transform, visible)),
            );
        }
        if !visible {
            continue;
        }
        if let Some(sprite) = node.sprite {
            if sprite.color.alpha() <= 0. {
                continue;
            }
            if let Some(part) = sprite_part(
                sprite,
                node.anchor.copied().unwrap_or_default(),
                transform,
                &images,
                &atlases,
                &mut textures,
            ) {
                avatar.push(part);
            }
        } else if let (Some(mesh), Some(material)) = (node.mesh, node.material)
            && let Some(material) = materials.get(&material.0)
            && material.color.alpha() > 0.
            && let Some(shape) = avatar.geometry(mesh.0.id(), &meshes)
        {
            let texture = match material.texture.as_ref() {
                Some(image) if images.contains(image.id()) => {
                    textures.add_image(EguiTextureHandle::Weak(image.id()))
                }
                Some(_) => continue,
                None => egui::TextureId::default(),
            };
            avatar.push(Part {
                shape: Shape::Mesh(shape),
                transform,
                texture,
                tint: material.color.to_linear(),
                uv_transform: material.uv_transform,
            });
        }
    }
    avatar.restore_unseen(&mut commands);
    // Keep only topology used by this pose. Dormant or replaced equipment
    // cannot fill the cache across a long visit; unchanged pieces keep their Arc.
    avatar.meshes.retain(|_, mesh| Arc::strong_count(mesh) > 1);
    let CrabAvatar { parts, order, .. } = &mut *avatar;
    order.sort_by(|&a, &b| {
        parts[a]
            .transform
            .translation
            .z
            .total_cmp(&parts[b].transform.translation.z)
            .then(a.cmp(&b))
    });
}

fn sprite_part(
    sprite: &Sprite,
    anchor: Anchor,
    transform: Affine3A,
    images: &Assets<Image>,
    atlases: &Assets<TextureAtlasLayout>,
    textures: &mut EguiUserTextures,
) -> Option<Part> {
    let solid = sprite.image == Handle::default();
    let image_size = if solid {
        Vec2::ONE
    } else {
        images.get(&sprite.image)?.size().as_vec2()
    };
    let atlas_rect = sprite
        .texture_atlas
        .as_ref()
        .and_then(|atlas| atlas.texture_rect(atlases))
        .map(|rect| rect.as_rect());
    let rect = match (atlas_rect, sprite.rect) {
        (None, None) => Rect::from_corners(Vec2::ZERO, image_size),
        (None, Some(rect)) | (Some(rect), None) => rect,
        (Some(atlas), Some(rect)) => Rect::from_corners(atlas.min + rect.min, atlas.min + rect.max),
    };
    let size = sprite.custom_size.unwrap_or_else(|| rect.size());
    if !size.is_finite()
        || size.min_element() <= 0.
        || !rect.min.is_finite()
        || !rect.max.is_finite()
    {
        return None;
    }
    let center = -anchor.as_vec() * size;
    let corners = [
        Vec2::new(-0.5, -0.5),
        Vec2::new(0.5, -0.5),
        Vec2::new(0.5, 0.5),
        Vec2::new(-0.5, 0.5),
    ];
    let shape = Shape::Quad(corners.map(|corner| {
        let uv = Vec2::new(
            if sprite.flip_x {
                0.5 - corner.x
            } else {
                0.5 + corner.x
            },
            if sprite.flip_y {
                0.5 + corner.y
            } else {
                0.5 - corner.y
            },
        );
        Vertex {
            position: (center + corner * size).extend(0.),
            uv: if solid {
                Vec2::new(egui::epaint::WHITE_UV.x, egui::epaint::WHITE_UV.y)
            } else {
                (rect.min + uv * rect.size()) / image_size
            },
            color: LinearRgba::WHITE,
        }
    }));
    Some(Part {
        shape,
        transform,
        texture: if solid {
            egui::TextureId::default()
        } else {
            textures.add_image(EguiTextureHandle::Weak(sprite.image.id()))
        },
        tint: sprite.color.to_linear(),
        uv_transform: Affine2::IDENTITY,
    })
}

fn source_geometry(mesh: &Mesh) -> Option<Geometry> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    if positions.is_empty() || positions.len() > MAX_MESH_VERTICES {
        return None;
    }
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uvs)) => Some(uvs),
        _ => None,
    };
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(colors)) => Some(colors),
        _ => None,
    };
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) if indices.len() <= MAX_INDICES => indices.iter().map(|i| i as u32).collect(),
        None => (0..positions.len() as u32).collect(),
        _ => return None,
    };
    if !indices.len().is_multiple_of(3)
        || indices
            .iter()
            .any(|&index| index as usize >= positions.len())
    {
        return None;
    }
    let mut bounds = Rect::from_corners(
        Vec2::from_slice(&positions[0]),
        Vec2::from_slice(&positions[0]),
    );
    let mut vertices = Vec::with_capacity(positions.len());
    for (index, position) in positions.iter().enumerate() {
        let position = Vec3::from_array(*position);
        if !position.is_finite() || position.abs().max_element() > 1_000_000. {
            return None;
        }
        bounds.min = bounds.min.min(position.truncate());
        bounds.max = bounds.max.max(position.truncate());
        let uv = uvs.and_then(|uvs| uvs.get(index)).copied().map_or(
            Vec2::new(egui::epaint::WHITE_UV.x, egui::epaint::WHITE_UV.y),
            Vec2::from_array,
        );
        let color = colors
            .and_then(|colors| colors.get(index))
            .copied()
            .map_or(LinearRgba::WHITE, LinearRgba::from_f32_array);
        if !uv.is_finite() || !color.to_f32_array().iter().all(|c| c.is_finite()) {
            return None;
        }
        vertices.push(Vertex {
            position,
            uv,
            color,
        });
    }
    Some(Geometry {
        vertices,
        indices,
        bounds,
    })
}

fn color32(color: LinearRgba) -> egui::Color32 {
    let [r, g, b, a] = color.to_f32_array();
    let [r, g, b, a] = Color::linear_rgba(r, g, b, a).to_srgba().to_u8_array();
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{asset::RenderAssetUsages, render::render_resource::Extent3d};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<TextureAtlasLayout>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ColorMaterial>>()
            .init_resource::<EguiUserTextures>()
            .add_message::<AssetEvent<Mesh>>()
            .insert_resource(super::super::Valmora {
                active: true,
                ..default()
            })
            .add_plugins(CrabAvatarPlugin);
        app
    }

    fn root(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                CrabAvatarSource,
                Transform::default(),
                Visibility::Inherited,
            ))
            .id()
    }

    fn painted(
        avatar: &CrabAvatar,
        feet: egui::Pos2,
        width: f32,
    ) -> Vec<egui::epaint::ClippedShape> {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui
                .ctx()
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(egui::Rect::from_min_max(
                    egui::Pos2::ZERO,
                    egui::pos2(400., 400.),
                ));
            avatar.paint(&painter, feet, width);
        });
        output.textures_delta.clear();
        output.shapes
    }

    #[test]
    fn projection_never_moves_or_copies_authoritative_crabs() {
        let mut app = app();
        let mut sim = crabsim::Sim::new(crabsim::Tuning::default(), crabsim::Level::test_level());
        sim.add_crab(crabsim::PlayerId(0), Vec2::new(2300., 500.));
        sim.add_crab(crabsim::PlayerId(7), Vec2::new(2400., 510.));
        let before = sim.state_hash();
        app.insert_resource(sim);
        let crab = app
            .world_mut()
            .spawn((
                crabsim::Mover::new(Vec2::new(2300., 500.)),
                Transform::from_xyz(2300., 500., 9.),
                Visibility::Hidden,
            ))
            .id();
        let root = root(&mut app);
        app.world_mut().entity_mut(root).insert(ChildOf(crab));
        let piece = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::new(20., 40.)),
                Transform::from_xyz(0., 20., 0.),
            ))
            .id();
        // Another character is never included, even if it is visible nearby.
        app.world_mut().spawn((
            Sprite::from_color(Color::WHITE, Vec2::splat(1000.)),
            Transform::default(),
        ));
        app.update();
        let avatar = app.world().resource::<CrabAvatar>();
        assert!(avatar.is_ready());
        assert_eq!(avatar.parts.len(), 1);
        assert_eq!(avatar.bounds.unwrap(), Rect::new(-10., 0., 10., 40.));
        let output = painted(avatar, egui::pos2(200., 200.), 80.);
        let mesh = output
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
        assert!(
            mesh.vertices
                .iter()
                .any(|v| v.pos == egui::pos2(160., 200.))
        );
        assert!(mesh.vertices.iter().any(|v| v.pos == egui::pos2(240., 40.)));
        assert_eq!(app.world().resource::<crabsim::Sim>().state_hash(), before);
        assert_eq!(
            app.world().get::<crabsim::Mover>(crab).unwrap().pos,
            Vec2::new(2300., 500.)
        );
        assert_eq!(
            app.world().get::<Transform>(crab).unwrap().translation,
            Vec3::new(2300., 500., 9.)
        );
        assert_eq!(
            app.world().get::<Transform>(piece).unwrap().translation,
            Vec3::new(0., 20., 0.)
        );
        assert_eq!(
            *app.world().get::<Visibility>(crab).unwrap(),
            Visibility::Hidden
        );
        // Repositioning only the presentation cannot affect the sim or source rig.
        let _ = painted(avatar, egui::pos2(100., 350.), 32.);
        assert_eq!(app.world().resource::<crabsim::Sim>().state_hash(), before);
    }

    #[test]
    fn atlas_rect_anchor_flip_and_texture_identity_are_preserved() {
        let mut images = Assets::<Image>::default();
        let mut image = Image::default();
        image.resize(Extent3d {
            width: 128,
            height: 64,
            depth_or_array_layers: 1,
        });
        let handle = images.add(image);
        let atlases = Assets::<TextureAtlasLayout>::default();
        let mut textures = EguiUserTextures::default();
        let mut sprite = Sprite {
            image: handle.clone(),
            rect: Some(Rect::new(32., 16., 64., 48.)),
            custom_size: Some(Vec2::new(20., 40.)),
            ..default()
        };
        let first = sprite_part(
            &sprite,
            Anchor::TOP_LEFT,
            Affine3A::IDENTITY,
            &images,
            &atlases,
            &mut textures,
        )
        .unwrap();
        assert_eq!(first.shape.vertices()[0].position, Vec3::new(0., -40., 0.));
        assert_eq!(first.shape.vertices()[0].uv, Vec2::new(0.25, 0.75));
        assert_eq!(first.shape.vertices()[2].uv, Vec2::new(0.5, 0.25));
        sprite.flip_x = true;
        sprite.flip_y = true;
        let flipped = sprite_part(
            &sprite,
            Anchor::TOP_LEFT,
            Affine3A::IDENTITY,
            &images,
            &atlases,
            &mut textures,
        )
        .unwrap();
        assert_eq!(flipped.shape.vertices()[0].uv, Vec2::new(0.5, 0.25));
        assert_eq!(flipped.texture, first.texture);
        assert_eq!(textures.image_id(handle.id()), Some(first.texture));
        assert_eq!(images.len(), 1, "projection must not upload a second atlas");
    }

    #[test]
    fn gear_and_missing_limbs_follow_current_hierarchy_visibility() {
        let mut app = app();
        let root = root(&mut app);
        let limb = app
            .world_mut()
            .spawn((ChildOf(root), Transform::default(), Visibility::Hidden))
            .id();
        app.world_mut().spawn((
            ChildOf(limb),
            Sprite::from_color(Color::WHITE, Vec2::splat(10.)),
        ));
        let gear = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::splat(20.)),
                Visibility::Hidden,
            ))
            .id();
        app.update();
        assert!(!app.world().resource::<CrabAvatar>().is_ready());
        app.world_mut()
            .entity_mut(gear)
            .insert(Visibility::Inherited);
        app.update();
        assert_eq!(app.world().resource::<CrabAvatar>().parts.len(), 1);
        // An explicitly Visible descendant behaves like Bevy, even when its
        // parent is hidden. Ordinary atlas layers inherit and stay hidden.
        app.world_mut().spawn((
            ChildOf(limb),
            Sprite::from_color(Color::WHITE, Vec2::splat(4.)),
            Visibility::Visible,
        ));
        app.update();
        assert_eq!(app.world().resource::<CrabAvatar>().parts.len(), 2);
        app.world_mut().entity_mut(gear).insert(Visibility::Hidden);
        app.update();
        assert_eq!(app.world().resource::<CrabAvatar>().parts.len(), 1);
    }

    #[test]
    fn carried_meshes_are_cached_and_solid_uvs_remain_white() {
        let mut app = app();
        let root = root(&mut app);
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(20., 10.));
        let material = app
            .world_mut()
            .resource_mut::<Assets<ColorMaterial>>()
            .add(Color::srgb(0.6, 0.2, 0.1));
        app.world_mut().spawn((
            ChildOf(root),
            Mesh2d(mesh.clone()),
            MeshMaterial2d(material),
            Transform::from_xyz(0., 10., 0.),
        ));
        app.update();
        let cached = Arc::clone(
            app.world()
                .resource::<CrabAvatar>()
                .meshes
                .get(&mesh.id())
                .unwrap(),
        );
        app.update();
        let avatar = app.world().resource::<CrabAvatar>();
        assert!(Arc::ptr_eq(&cached, avatar.meshes.get(&mesh.id()).unwrap()));
        let output = painted(avatar, egui::pos2(200., 200.), 80.);
        for shape in &output {
            if let egui::Shape::Mesh(mesh) = &shape.shape {
                assert!(mesh.vertices.iter().all(|v| v.uv == egui::epaint::WHITE_UV));
            }
        }
        app.world_mut()
            .write_message(AssetEvent::<Mesh>::Modified { id: mesh.id() });
        app.update();
        assert!(!Arc::ptr_eq(
            &cached,
            app.world()
                .resource::<CrabAvatar>()
                .meshes
                .get(&mesh.id())
                .unwrap()
        ));
    }

    #[test]
    fn malformed_and_oversized_meshes_are_not_projected() {
        use bevy::mesh::Indices;
        let mesh = || {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD,
            )
        };
        let large = mesh().with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0., 0., 0.]; MAX_MESH_VERTICES + 1],
        );
        assert!(source_geometry(&large).is_none());
        let invalid = mesh()
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0., 0., 0.]; 3])
            .with_inserted_indices(Indices::U32(vec![0, 1, 9]));
        assert!(source_geometry(&invalid).is_none());
        let nonfinite =
            mesh().with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[f32::NAN, 0., 0.]; 3]);
        assert!(source_geometry(&nonfinite).is_none());
    }

    #[test]
    fn travel_hides_only_source_rendering_and_restores_exact_layers() {
        let mut app = app();
        let root = root(&mut app);
        let plain = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::splat(10.)),
            ))
            .id();
        let custom_layers = RenderLayers::from_layers(&[2, 5]);
        let custom = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::splat(10.)),
                custom_layers.clone(),
            ))
            .id();
        let hidden = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::splat(10.)),
                RenderLayers::none(),
            ))
            .id();
        let other = app
            .world_mut()
            .spawn(Sprite::from_color(Color::WHITE, Vec2::splat(10.)))
            .id();
        app.update();
        for entity in [plain, custom, hidden] {
            assert_eq!(
                app.world().get::<RenderLayers>(entity),
                Some(&RenderLayers::none())
            );
        }
        assert!(app.world().get::<RenderLayers>(other).is_none());
        assert_eq!(
            app.world().resource::<CrabAvatar>().parts.len(),
            3,
            "source render suppression must not suppress the projection"
        );
        // A cosmetic reparented out of the puppet immediately regains its
        // original layers; the bounded cache does not retain former descendants.
        app.world_mut().entity_mut(custom).remove::<ChildOf>();
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(custom),
            Some(&custom_layers)
        );
        assert_eq!(app.world().resource::<CrabAvatar>().source_layers.len(), 2);
        let mut state = app.world_mut().resource_mut::<super::super::Valmora>();
        state.active = false;
        state.release_guard = true;
        app.update();
        assert!(app.world().get::<RenderLayers>(plain).is_none());
        assert_eq!(
            app.world().get::<RenderLayers>(hidden),
            Some(&RenderLayers::none())
        );
        assert!(
            app.world()
                .resource::<CrabAvatar>()
                .source_layers
                .is_empty()
        );
        assert!(!app.world().resource::<CrabAvatar>().is_ready());
    }

    #[test]
    fn disappearing_source_restores_its_pictures_without_a_second_crab() {
        let mut app = app();
        let root = root(&mut app);
        let piece = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::splat(10.)),
                RenderLayers::layer(4),
            ))
            .id();
        app.update();
        app.world_mut()
            .entity_mut(root)
            .remove::<CrabAvatarSource>();
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(piece),
            Some(&RenderLayers::layer(4))
        );
        assert!(!app.world().resource::<CrabAvatar>().is_ready());
        assert!(
            app.world()
                .resource::<CrabAvatar>()
                .source_layers
                .is_empty()
        );
    }

    #[test]
    fn oversized_hierarchies_and_repeated_draws_remain_bounded() {
        let mut app = app();
        let root = root(&mut app);
        for _ in 0..(MAX_NODES * 2) {
            app.world_mut().spawn((
                ChildOf(root),
                Sprite::from_color(Color::WHITE, Vec2::splat(10.)),
            ));
        }
        for _ in 0..3 {
            app.update();
            let avatar = app.world().resource::<CrabAvatar>();
            assert_eq!(avatar.parts.len(), MAX_PARTS);
            assert!(avatar.stack.len() <= MAX_NODES);
            assert!(avatar.source_layers.len() <= MAX_NODES);
            assert!(avatar.vertices <= MAX_VERTICES);
            assert!(avatar.indices <= MAX_INDICES);
            let output = painted(avatar, egui::pos2(200., 200.), 80.);
            let vertices: usize = output
                .iter()
                .map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => mesh.vertices.len(),
                    _ => 0,
                })
                .sum();
            assert_eq!(vertices, MAX_PARTS * 4);
        }
        let avatar = app.world().resource::<CrabAvatar>();
        assert!(
            painted(avatar, egui::pos2(-200., -200.), 80.)
                .iter()
                .all(|shape| !matches!(shape.shape, egui::Shape::Mesh(_)))
        );
        assert!(
            painted(avatar, egui::pos2(200., 200.), f32::NAN)
                .iter()
                .all(|shape| !matches!(shape.shape, egui::Shape::Mesh(_)))
        );
        app.world_mut()
            .resource_mut::<super::super::Valmora>()
            .active = false;
        app.update();
        let avatar = app.world().resource::<CrabAvatar>();
        assert!(!avatar.is_ready());
        assert!(avatar.parts.is_empty() && avatar.meshes.is_empty());
    }
}
