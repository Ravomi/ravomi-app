//! Clip every node drawing layer against its actual ancestor group silhouettes.
//! Source sprites stay intact for layout/input. Their render extraction is replaced
//! with meshes using the same slices, texture and transform, plus a shared GPU mask.
use super::*;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::{Indices, PrimitiveTopology},
    render::{
        Extract, ExtractSchedule, RenderApp,
        render_resource::{AsBindGroup, ShaderType},
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite::{SpriteImageMode, TextureSlice},
    sprite_render::{AlphaMode2d, ExtractedSprites, Material2d, Material2dPlugin, SpriteSystems},
};

#[derive(Clone, Copy, Default, PartialEq, ShaderType)]
struct Region {
    bounds: Vec4,
    radius: f32,
    parent: u32,
    padding: Vec2,
}

#[derive(Clone, Copy, ShaderType, Debug)]
struct Settings {
    head: u32,
    pixel_range: f32,
    padding: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
struct ClipMaterial {
    #[texture(0)]
    #[sampler(1)]
    content: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    mask: Handle<Image>,
    #[storage(4, read_only)]
    regions: Handle<ShaderBuffer>,
    #[uniform(5)]
    settings: Settings,
}

impl Material2d for ClipMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://project_graph_bevy_canvas/group_clip.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

type MaterialKey = (AssetId<Image>, u32, u32);

#[derive(Resource, Default)]
struct Clips {
    buffer: Handle<ShaderBuffer>,
    regions: Vec<Region>,
    heads: HashMap<String, u32>,
    owners: HashMap<Entity, u32>,
    materials: HashMap<MaterialKey, Handle<ClipMaterial>>,
}

#[derive(Component)]
struct ReplacedSprite {
    parts: Vec<Entity>,
    head: u32,
    sprite: Option<SpriteState>,
}

#[derive(PartialEq)]
struct SpriteState {
    image: AssetId<Image>,
    rect: Rect,
    size: Vec2,
    anchor: Vec2,
    color: Color,
    flip: [bool; 2],
    mode: SpriteImageMode,
}

#[derive(Component, Clone)]
enum OriginalMaterial {
    Msdf(Handle<msdf_text::MsdfMaterial>),
    Stroke(Handle<edges::EdgeMaterial>),
}

pub fn install(app: &mut App) {
    embedded_asset!(app, "group_clip.wgsl");
    app.init_resource::<Clips>()
        .add_plugins(Material2dPlugin::<ClipMaterial>::default())
        .add_systems(
            PostUpdate,
            (update_regions, sprites, mesh_materials, fallback_text)
                .chain()
                .after(CanvasVisuals)
                .after(bevy::sprite::update_text2d_layout)
                .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds)
                .before(bevy::transform::TransformSystems::Propagate),
        );
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.add_systems(
            ExtractSchedule,
            remove_replaced_sprites.after(SpriteSystems::ExtractSprites),
        );
    }
}

fn remove_replaced_sprites(
    replaced: Extract<Query<(), With<ReplacedSprite>>>,
    mut sprites: ResMut<ExtractedSprites>,
) {
    sprites
        .sprites
        .retain(|sprite| !replaced.contains(sprite.main_entity));
}

fn build_regions(
    graph: &Graph,
    scale: Option<f32>,
    positions: &HashMap<usize, Vec2>,
) -> (Vec<Region>, HashMap<String, u32>) {
    let parents: HashMap<_, _> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == "Section" && !n.collapsed)
        .flat_map(|(i, n)| n.children.iter().map(move |child| (child.as_str(), i)))
        .collect();
    let mut regions = vec![Region::default()];
    let mut indices = HashMap::new();
    // Walk each chain once, inserting parents before descendants. This supports
    // arbitrary nesting without a per-material fixed-size ancestor array.
    for (index, _) in graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == "Section" && !n.collapsed)
    {
        let mut chain = Vec::new();
        let mut visiting = HashSet::new();
        let mut current = index;
        while !indices.contains_key(&current) && visiting.insert(current) {
            chain.push(current);
            let Some(&parent) = parents.get(graph.nodes[current].id.as_str()) else {
                break;
            };
            current = parent;
        }
        for index in chain.into_iter().rev() {
            let node = &graph.nodes[index];
            let size = Vec2::new(node.rect[2], node.rect[3]);
            let center = positions
                .get(&index)
                .copied()
                .unwrap_or_else(|| center(node));
            let parent = parents
                .get(node.id.as_str())
                .and_then(|i| indices.get(i))
                .copied()
                .unwrap_or(0);
            indices.insert(index, regions.len() as u32);
            regions.push(Region {
                bounds: Vec4::new(center.x, center.y, size.x * 0.5, size.y * 0.5),
                radius: scale.map_or_else(
                    || theme::corners::radius(size) as f32,
                    |s| theme::corners::group_radius(size, s),
                ),
                parent,
                padding: Vec2::ZERO,
            });
        }
    }
    let heads = graph
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.clone(),
                parents
                    .get(node.id.as_str())
                    .and_then(|i| indices.get(i))
                    .copied()
                    .unwrap_or(0),
            )
        })
        .collect();
    (regions, heads)
}

#[allow(clippy::too_many_arguments)]
fn update_regions(
    scene: Res<Scene>,
    display: Res<sections::Display>,
    camera: Query<&Projection, With<Camera2d>>,
    export: Option<Res<export_image::ExportImage>>,
    roots: Query<(Entity, &NodeVisual, &Transform)>,
    overviews: Query<(Entity, &section_overview::OverviewVisual)>,
    active_materials: Query<&MeshMaterial2d<ClipMaterial>>,
    mut clips: ResMut<Clips>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<ClipMaterial>>,
) {
    let graph = display.graph(&scene);
    let positions = roots
        .iter()
        .map(|(_, n, t)| (n.0, t.translation.truncate()))
        .collect();
    let scale = theme::group_scale(camera.single().ok(), export.is_some());
    let (regions, heads) = build_regions(graph, scale, &positions);
    if clips.regions != regions {
        if clips.regions.len() == regions.len()
            && let Some(mut buffer) = buffers.get_mut(&clips.buffer)
        {
            buffer.set_data(regions.clone());
        } else {
            // A size change reallocates the GPU buffer. Existing material bind
            // groups retain the old allocation unless the material is updated.
            // A fresh handle also makes preparation wait for the new GPU asset.
            let previous = clips.buffer.clone();
            clips.buffer = buffers.add(ShaderBuffer::from(regions.clone()));
            for (_, material) in materials.iter_mut() {
                if material.regions == previous {
                    material.regions = clips.buffer.clone();
                }
            }
        }
        clips.regions = regions;
    }
    clips.heads = heads;
    clips.owners.clear();
    for (entity, node, _) in &roots {
        if let Some(node) = graph.nodes.get(node.0) {
            let head = clips.heads[&node.id];
            clips.owners.insert(entity, head);
        }
    }
    for (entity, overview) in &overviews {
        let head = clips.heads.get(&overview.id).copied().unwrap_or(0);
        clips.owners.insert(entity, head);
    }
    // Release unused per-group material handles after regrouping/document changes.
    let live: HashSet<_> = clips.heads.values().copied().collect();
    let used: HashSet<_> = active_materials.iter().map(|m| m.0.id()).collect();
    clips
        .materials
        .retain(|(_, head, _), material| live.contains(head) && used.contains(&material.id()));
}

impl Clips {
    fn head(&self, mut entity: Entity, parents: &Query<&ChildOf>) -> u32 {
        loop {
            if let Some(&head) = self.owners.get(&entity) {
                return head;
            }
            let Ok(parent) = parents.get(entity) else {
                return 0;
            };
            entity = parent.parent();
        }
    }
    fn material(
        &mut self,
        image: Handle<Image>,
        head: u32,
        range: f32,
        mask: &Handle<Image>,
        materials: &mut Assets<ClipMaterial>,
    ) -> Handle<ClipMaterial> {
        self.materials
            .entry((image.id(), head, range.to_bits()))
            .or_insert_with(|| {
                materials.add(ClipMaterial {
                    content: image,
                    mask: mask.clone(),
                    regions: self.buffer.clone(),
                    settings: Settings {
                        head,
                        pixel_range: range,
                        padding: Vec2::ZERO,
                    },
                })
            })
            .clone()
    }
}

#[derive(Default)]
struct Quads {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Quads {
    fn push(&mut self, center: Vec2, size: Vec2, uv: Rect, color: Color, flip: [bool; 2]) {
        let i = self.positions.len() as u32;
        let half = size * 0.5;
        for p in [
            Vec2::new(-half.x, half.y),
            half,
            Vec2::new(half.x, -half.y),
            -half,
        ] {
            self.positions.push((center + p).extend(0.0).to_array());
        }
        let [left, right] = if flip[0] {
            [uv.max.x, uv.min.x]
        } else {
            [uv.min.x, uv.max.x]
        };
        let [top, bottom] = if flip[1] {
            [uv.max.y, uv.min.y]
        } else {
            [uv.min.y, uv.max.y]
        };
        self.uvs
            .extend([[left, top], [right, top], [right, bottom], [left, bottom]]);
        self.colors.extend([color.to_linear().to_f32_array(); 4]);
        self.indices.extend([i, i + 2, i + 1, i, i + 3, i + 2]);
    }
    fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sprites(
    mut commands: Commands,
    parents: Query<&ChildOf>,
    assets: Res<NodeAssets>,
    sprites: Query<(Entity, Ref<Sprite>, &Anchor, Option<&ReplacedSprite>)>,
    proxy_meshes: Query<&Mesh2d>,
    images: Res<Assets<Image>>,
    atlases: Res<Assets<TextureAtlasLayout>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ClipMaterial>>,
    mut clips: ResMut<Clips>,
) {
    let mask = assets.corners.clip_mask();
    for (entity, sprite, anchor, old) in &sprites {
        let head = clips.head(entity, &parents);
        if head == 0 {
            if let Some(old) = old {
                for &part in &old.parts {
                    commands.entity(part).despawn();
                }
                commands.entity(entity).remove::<ReplacedSprite>();
            }
            continue;
        }
        let Some(image) = images.get(&sprite.image) else {
            continue;
        };
        let image_size = image.size_f32();
        let rect = sprite
            .rect
            .or_else(|| {
                sprite
                    .texture_atlas
                    .as_ref()
                    .and_then(|atlas| atlas.texture_rect(&atlases))
                    .map(|r| r.as_rect())
            })
            .unwrap_or(Rect::from_corners(Vec2::ZERO, image_size));
        let size = sprite.custom_size.unwrap_or(rect.size());
        let state = SpriteState {
            image: sprite.image.id(),
            rect,
            size,
            anchor: anchor.as_vec(),
            color: sprite.color,
            flip: [sprite.flip_x, sprite.flip_y],
            mode: sprite.image_mode.clone(),
        };
        if old.is_some_and(|old| old.head == head && old.sprite.as_ref() == Some(&state)) {
            continue;
        }
        let slices = if let SpriteImageMode::Sliced(slicer) = &sprite.image_mode {
            slicer.compute_slices(rect, Some(size))
        } else {
            vec![TextureSlice {
                texture_rect: rect,
                draw_size: size,
                offset: Vec2::ZERO,
            }]
        };
        let mut quads = Quads::default();
        for slice in slices {
            let mut offset = slice.offset;
            if sprite.flip_x {
                offset.x = -offset.x;
            }
            if sprite.flip_y {
                offset.y = -offset.y;
            }
            quads.push(
                offset - anchor.as_vec() * size,
                slice.draw_size,
                Rect::from_corners(
                    slice.texture_rect.min / image_size,
                    slice.texture_rect.max / image_size,
                ),
                sprite.color,
                [sprite.flip_x, sprite.flip_y],
            );
        }
        let material = clips.material(sprite.image.clone(), head, 0.0, &mask, &mut materials);
        // Keep entities and mesh handles stable while zooming; only the small
        // nine-slice geometry changes. Text glyph meshes are never rebuilt here.
        let part = if let Some(part) = old.and_then(|old| old.parts.first()).copied()
            && let Ok(handle) = proxy_meshes.get(part)
            && let Some(mut mesh) = meshes.get_mut(&handle.0)
        {
            *mesh = quads.mesh();
            // Refresh frustum bounds now, without waiting for next frame's
            // mesh asset event. The mesh and entity handles remain unchanged.
            commands
                .entity(part)
                .insert((Mesh2d(handle.0.clone()), MeshMaterial2d(material)));
            part
        } else {
            commands
                .spawn((
                    Mesh2d(meshes.add(quads.mesh())),
                    MeshMaterial2d(material),
                    Transform::default(),
                    ChildOf(entity),
                ))
                .id()
        };
        commands.entity(entity).insert(ReplacedSprite {
            parts: vec![part],
            head,
            sprite: Some(state),
        });
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn mesh_materials(
    mut commands: Commands,
    parents: Query<&ChildOf>,
    assets: Res<NodeAssets>,
    parts: Query<
        (
            Entity,
            Option<&MeshMaterial2d<msdf_text::MsdfMaterial>>,
            Option<&MeshMaterial2d<edges::EdgeMaterial>>,
            Option<&OriginalMaterial>,
            Option<&MeshMaterial2d<ClipMaterial>>,
        ),
        Or<(
            With<MeshMaterial2d<msdf_text::MsdfMaterial>>,
            With<MeshMaterial2d<edges::EdgeMaterial>>,
            With<OriginalMaterial>,
        )>,
    >,
    source: Res<Assets<msdf_text::MsdfMaterial>>,
    mut materials: ResMut<Assets<ClipMaterial>>,
    mut clips: ResMut<Clips>,
) {
    let mask = assets.corners.clip_mask();
    for (entity, msdf, stroke, old, current) in &parts {
        let Some(original) = msdf
            .map(|m| OriginalMaterial::Msdf(m.0.clone()))
            .or_else(|| stroke.map(|m| OriginalMaterial::Stroke(m.0.clone())))
            .or_else(|| old.cloned())
        else {
            continue;
        };
        let head = clips.head(entity, &parents);
        if head == 0 {
            if old.is_some() {
                let mut entity = commands.entity(entity);
                match original {
                    OriginalMaterial::Msdf(handle) => {
                        entity.insert(MeshMaterial2d(handle));
                    }
                    OriginalMaterial::Stroke(handle) => {
                        entity.insert(MeshMaterial2d(handle));
                    }
                }
                entity.remove::<(OriginalMaterial, MeshMaterial2d<ClipMaterial>)>();
            }
            continue;
        }
        let (image, range) = match &original {
            OriginalMaterial::Msdf(handle) => {
                let Some(source) = source.get(handle) else {
                    continue;
                };
                (source.atlas.clone(), source.pixel_range)
            }
            // Node-owned edge meshes are freehand strokes (always solid).
            // Graph edges have no node ancestor and retain their normal material.
            OriginalMaterial::Stroke(_) => (Handle::default(), 0.0),
        };
        let material = clips.material(image, head, range, &mask, &mut materials);
        if current.is_none_or(|current| current.0 != material) {
            commands
                .entity(entity)
                .insert((original, MeshMaterial2d(material)))
                .remove::<(
                    MeshMaterial2d<msdf_text::MsdfMaterial>,
                    MeshMaterial2d<edges::EdgeMaterial>,
                )>();
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn fallback_text(
    mut commands: Commands,
    parents: Query<&ChildOf>,
    assets: Res<NodeAssets>,
    labels: Query<
        (
            Entity,
            Ref<bevy::text::TextLayoutInfo>,
            Ref<bevy::text::TextBounds>,
            Ref<Anchor>,
            Ref<TextColor>,
            Option<&ReplacedSprite>,
        ),
        With<Text2d>,
    >,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ClipMaterial>>,
    mut clips: ResMut<Clips>,
) {
    let mask = assets.corners.clip_mask();
    for (entity, info, bounds, anchor, color, old) in &labels {
        let head = clips.head(entity, &parents);
        if head == 0 {
            if let Some(old) = old {
                for &part in &old.parts {
                    commands.entity(part).despawn();
                }
                commands.entity(entity).remove::<ReplacedSprite>();
            }
            continue;
        }
        if old.is_some_and(|old| old.head == head)
            && !info.is_changed()
            && !color.is_changed()
            && !bounds.is_changed()
            && !anchor.is_changed()
        {
            continue;
        }
        let size = Vec2::new(
            bounds.width.unwrap_or(info.size.x),
            bounds.height.unwrap_or(info.size.y),
        );
        let origin = (Anchor::TOP_LEFT.as_vec() - anchor.as_vec()) * size;
        let scale = info.scale_factor.recip();
        let mut pages: HashMap<AssetId<Image>, Quads> = HashMap::new();
        for glyph in &info.glyphs {
            let atlas = &glyph.atlas_info;
            let Some(image) = images.get(atlas.texture) else {
                continue;
            };
            pages.entry(atlas.texture).or_default().push(
                origin + Vec2::new(glyph.position.x, -glyph.position.y) * scale,
                atlas.rect.size() * scale,
                Rect::from_corners(
                    atlas.rect.min / image.size_f32(),
                    atlas.rect.max / image.size_f32(),
                ),
                color.0,
                [false, false],
            );
        }
        if let Some(old) = old {
            for &part in &old.parts {
                commands.entity(part).despawn();
            }
        }
        let parts = pages
            .into_iter()
            .map(|(image, quads)| {
                let material = clips.material(
                    images.get_strong_handle(image).expect("live glyph atlas"),
                    head,
                    0.0,
                    &mask,
                    &mut materials,
                );
                commands
                    .spawn((
                        Mesh2d(meshes.add(quads.mesh())),
                        MeshMaterial2d(material),
                        Transform::default(),
                        ChildOf(entity),
                    ))
                    .id()
            })
            .collect();
        commands.entity(entity).insert(ReplacedSprite {
            parts,
            head,
            sprite: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizing_regions_rebinds_existing_materials_and_movement_reuses_buffer() {
        let mut doc = Document::blank().unwrap();
        let leaf = doc.create_text_node([100.0, 100.0]).unwrap();
        let group = doc.create_group(&HashSet::from([leaf])).unwrap();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .init_resource::<sections::Display>()
            .init_resource::<Clips>()
            .init_resource::<Assets<ShaderBuffer>>()
            .init_resource::<Assets<ClipMaterial>>()
            .add_systems(Update, update_regions);
        app.update();
        let first = app.world().resource::<Clips>().buffer.clone();
        let material = app
            .world_mut()
            .resource_mut::<Assets<ClipMaterial>>()
            .add(ClipMaterial {
                content: default(),
                mask: default(),
                regions: first.clone(),
                settings: Settings {
                    head: 1,
                    pixel_range: 0.0,
                    padding: Vec2::ZERO,
                },
            });
        app.world_mut().spawn(MeshMaterial2d(material.clone()));
        for collapsed in [true, false] {
            let previous = app.world().resource::<Clips>().buffer.clone();
            app.world_mut().resource_mut::<Scene>().0.nodes[group].collapsed = collapsed;
            app.update();
            let current = app.world().resource::<Clips>().buffer.clone();
            assert_ne!(
                current, previous,
                "resizing must invalidate the old GPU buffer binding"
            );
            assert_eq!(
                app.world()
                    .resource::<Assets<ClipMaterial>>()
                    .get(&material)
                    .unwrap()
                    .regions,
                current
            );
        }
        let current = app.world().resource::<Clips>().buffer.clone();
        app.world_mut().resource_mut::<Scene>().0.nodes[group].rect[0] += 200.0;
        app.update();
        assert_eq!(app.world().resource::<Clips>().buffer, current);
        let clips = app.world().resource::<Clips>();
        let scene = app.world().resource::<Scene>();
        assert_eq!(clips.regions[1].bounds.x, center(&scene.0.nodes[group]).x);
    }

    #[test]
    fn ancestors_follow_live_geometry_zoom_and_ungrouping() {
        let mut doc = Document::blank().unwrap();
        let leaf = doc.create_text_node([100.0, 100.0]).unwrap();
        let inner = doc.create_group(&HashSet::from([leaf])).unwrap();
        let outer = doc.create_group(&HashSet::from([inner])).unwrap();
        doc.nodes[inner].rect = [0.0, 0.0, 1000.0, 600.0];
        doc.nodes[outer].rect = [-50.0, -50.0, 1200.0, 800.0];
        let original_bounds = doc.nodes[outer].rect;
        let revision = doc.revision;
        for frame in 0..120 {
            let scale = 0.25 + (frame as f32 * 0.13).sin().abs() * 20.0;
            let position = Vec2::new(frame as f32 * 3.0, -40.0);
            let (regions, heads) =
                build_regions(&doc.graph, Some(scale), &HashMap::from([(outer, position)]));
            let leaf_head = heads[&doc.nodes[leaf].id] as usize;
            let outer_head = heads[&doc.nodes[inner].id] as usize;
            assert_ne!(leaf_head, 0);
            assert_eq!(regions[leaf_head].parent as usize, outer_head);
            assert_eq!(regions[outer_head].parent, 0);
            assert_eq!(regions[outer_head].bounds.truncate().truncate(), position);
            assert_eq!(regions[outer_head].radius, (24.0 * scale).min(200.0));
            assert_eq!(
                heads[&doc.nodes[outer].id], 0,
                "a group must not clip its own border"
            );
        }
        assert_eq!(doc.nodes[outer].rect, original_bounds);
        assert_eq!(doc.revision, revision);
        doc.nodes[outer].children.clear();
        let (_, heads) = build_regions(&doc.graph, Some(1.0), &HashMap::new());
        assert_eq!(heads[&doc.nodes[inner].id], 0);
        assert_ne!(heads[&doc.nodes[leaf].id], 0);
    }

    #[test]
    fn malformed_cycles_cannot_create_gpu_loops() {
        let mut doc = Document::blank().unwrap();
        let leaf = doc.create_text_node([0.0, 0.0]).unwrap();
        let inner = doc.create_group(&HashSet::from([leaf])).unwrap();
        let outer = doc.create_group(&HashSet::from([inner])).unwrap();
        let outer_id = doc.nodes[outer].id.clone();
        doc.nodes[inner].children.push(outer_id);
        let (regions, _) = build_regions(&doc.graph, Some(20.0), &HashMap::new());
        for (index, region) in regions.iter().enumerate().skip(1) {
            assert!((region.parent as usize) < index);
        }
    }

    #[test]
    fn zoom_reuses_sprite_and_glyph_meshes_and_ungrouping_restores_sources() {
        let mut images = Assets::<Image>::default();
        let image = images.add(Image::default());
        let corners = theme::NodeCorners::new(&mut images);
        let mut app = App::new();
        app.insert_resource(images)
            .insert_resource(NodeAssets {
                font: default(),
                corners,
                textures: default(),
            })
            .init_resource::<Assets<TextureAtlasLayout>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ClipMaterial>>()
            .init_resource::<Assets<msdf_text::MsdfMaterial>>()
            .init_resource::<Clips>()
            .add_systems(Update, (sprites, mesh_materials).chain());
        let root = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .resource_mut::<Clips>()
            .owners
            .insert(root, 1);
        let sprite = app
            .world_mut()
            .spawn((Sprite::from_image(image.clone()), ChildOf(root)))
            .id();
        let source = app
            .world_mut()
            .resource_mut::<Assets<msdf_text::MsdfMaterial>>()
            .add(msdf_text::MsdfMaterial {
                atlas: image,
                pixel_range: 4.0,
            });
        let glyph_mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(20.0, 20.0));
        let glyph = app
            .world_mut()
            .spawn((
                Mesh2d(glyph_mesh.clone()),
                MeshMaterial2d(source.clone()),
                ChildOf(root),
            ))
            .id();
        app.update();
        let proxy = app.world().get::<ReplacedSprite>(sprite).unwrap().parts[0];
        let proxy_mesh = app.world().get::<Mesh2d>(proxy).unwrap().0.id();
        for frame in 0..120 {
            app.world_mut()
                .get_mut::<Sprite>(sprite)
                .unwrap()
                .custom_size = Some(Vec2::splat(20.0 + frame as f32));
            app.update();
            assert_eq!(
                app.world().get::<ReplacedSprite>(sprite).unwrap().parts,
                [proxy]
            );
            assert_eq!(app.world().get::<Mesh2d>(proxy).unwrap().0.id(), proxy_mesh);
            assert_eq!(app.world().get::<Mesh2d>(glyph).unwrap().0, glyph_mesh);
            assert!(
                app.world()
                    .get::<MeshMaterial2d<msdf_text::MsdfMaterial>>(glyph)
                    .is_none()
            );
            assert!(
                app.world()
                    .get::<MeshMaterial2d<ClipMaterial>>(glyph)
                    .is_some()
            );
        }
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 2);
        app.world_mut().resource_mut::<Clips>().owners.clear();
        app.update();
        assert!(app.world().get::<ReplacedSprite>(sprite).is_none());
        assert!(app.world().get_entity(proxy).is_err());
        assert_eq!(
            app.world()
                .get::<MeshMaterial2d<msdf_text::MsdfMaterial>>(glyph)
                .unwrap()
                .0,
            source
        );
        assert!(
            app.world()
                .get::<MeshMaterial2d<ClipMaterial>>(glyph)
                .is_none()
        );
    }
}
