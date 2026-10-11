use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::{DynamicTextureAtlasBuilder, DynamicTextureAtlasBuilderError, ImageSampler},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite::Anchor,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
    text::{
        ComputedTextBlock, FontCx, LayoutCx, LetterSpacing, LineHeight, TextBounds, TextLayoutInfo,
        TextPipeline,
    },
};
use kayak_font_msdf::{GeneratedGlyph, GlyphBitmap, PIXEL_RANGE};
use parley::PositionedLayoutItem;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
};

const ATLAS_SIZE: u32 = 1024;
const MAX_PAGES: usize = 16;
const MAX_TASKS: usize = 4;
// A worker continues between frame polls instead of waiting for the render
// thread after every glyph. Bound both CPU concurrency and completed uploads.
const GLYPHS_PER_TASK: usize = 8;

#[derive(Component, Default)]
#[require(
    TextFont,
    TextLayout,
    TextColor,
    TextBounds,
    LineHeight,
    LetterSpacing,
    Anchor,
    Transform,
    Visibility,
    TextState
)]
pub struct MsdfText(pub String);

impl MsdfText {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

#[cfg(test)]
#[path = "msdf_text_tests.rs"]
pub(super) mod tests;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    face: u32,
    glyph: u16,
}

struct Placement {
    key: GlyphKey,
    baseline: Vec2,
    size: f32,
    color: Option<Color>,
}

#[derive(Component, Default)]
pub(super) struct TextState {
    initialized: bool,
    dirty: bool,
    failed: bool,
    color_fallback: bool,
    glyphs: Vec<Placement>,
    parts: Vec<Entity>,
    math: Option<ratex_types::display_item::DisplayList>,
}

#[derive(Clone, Copy)]
struct CachedGlyph {
    page: usize,
    rect: URect,
    plane: [f32; 4],
}

enum GlyphStatus {
    Pending,
    Ready(Option<CachedGlyph>),
    Color,
    Failed(String),
}

struct AtlasPage {
    image: Handle<Image>,
    material: Handle<MsdfMaterial>,
    layout: TextureAtlasLayout,
    builder: DynamicTextureAtlasBuilder,
}

type GlyphTask = Task<Vec<(GlyphKey, Result<GeneratedGlyph, String>)>>;

#[derive(Resource, Default)]
pub struct MsdfCache {
    fallbacks_initialized: bool,
    fonts: HashMap<u64, Arc<[u8]>>,
    font_aliases: HashMap<u64, u64>,
    glyphs: HashMap<GlyphKey, GlyphStatus>,
    queue: VecDeque<GlyphKey>,
    tasks: Vec<(usize, GlyphTask)>,
    pages: Vec<AtlasPage>,
    math_fonts: HashMap<String, u64>,
    solid: Option<Handle<MsdfMaterial>>,
    pub generated: usize,
    status: MsdfStatus,
}

fn configure_symbol_fallbacks(font_cx: &mut FontCx) {
    use parley::fontique::{Blob, FontInfoOverride, GenericFamily, Script, ScriptExt};
    let collection = &mut font_cx.context.collection;
    // Swash cannot paint the system Noto COLRv1 font. Use its CBDT edition
    // under a private family name, without changing the system font selection.
    const EMOJI_FAMILY: &str = "Graplu Color Emoji";
    if collection.family_id(EMOJI_FAMILY).is_none() {
        collection.register_fonts(
            Blob::from(include_bytes!("../assets/fonts/NotoColorEmoji.ttf").to_vec()),
            Some(FontInfoOverride {
                family_name: Some(EMOJI_FAMILY),
                ..default()
            }),
        );
    }
    if let Some(emoji) = collection.family_id(EMOJI_FAMILY) {
        collection.set_generic_families(GenericFamily::Emoji, std::iter::once(emoji));
    }
    let families: Vec<_> = [
        "Noto Sans Symbols 2",
        "Noto Sans Symbols",
        "DejaVu Sans",
        "Segoe UI Symbol",
        "Apple Symbols",
        "Arial Unicode MS",
    ]
    .into_iter()
    .filter_map(|name| collection.family_id(name))
    .collect();
    if families.is_empty() {
        return;
    }
    // Fontique 0.9 selects one system family per script on Linux. Symbols
    // inherit adjacent scripts, so preserve those defaults and append the
    // discovered symbol families to each script's fallback chain.
    for script in Script::all_samples()
        .iter()
        .map(|(script, _)| *script)
        .chain(std::iter::once(Script::from_bytes(*b"Zyyy")))
    {
        let existing: Vec<_> = collection.fallback_families(script).collect();
        collection.append_fallbacks(
            script,
            families.iter().copied().filter(|id| !existing.contains(id)),
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MsdfStatus {
    pub pending_glyphs: usize,
    pub pending_labels: usize,
    pub failed_labels: usize,
    pub color_labels: usize,
    pub generated_glyphs: usize,
    pub mesh_rebuilds: usize,
    pub atlas_pages: usize,
    pub atlas_bytes: usize,
}

impl Default for MsdfStatus {
    fn default() -> Self {
        Self {
            pending_glyphs: 0,
            pending_labels: 1,
            failed_labels: 0,
            color_labels: 0,
            generated_glyphs: 0,
            mesh_rebuilds: 0,
            atlas_pages: 0,
            atlas_bytes: 0,
        }
    }
}

impl MsdfStatus {
    pub fn ready(&self) -> bool {
        self.pending_glyphs == 0 && self.pending_labels == 0 && self.failed_labels == 0
    }
}

impl MsdfCache {
    fn math_font(&mut self, name: &str, fallback: Option<&Font>) -> Result<u64, String> {
        if name == "CJK-Regular" {
            let font = fallback.ok_or("公式中的中文需要已加载的界面字体")?;
            return Ok(self.register_font(font.data.id(), font.data.as_ref()));
        }
        if let Some(&id) = self.math_fonts.get(name) {
            return Ok(id);
        }
        let filename = format!("KaTeX_{name}.ttf");
        let bytes = ratex_katex_fonts::ttf_bytes(&filename)
            .ok_or_else(|| format!("公式字体缺失：{filename}"))?;
        let source = u64::MAX - self.math_fonts.len() as u64;
        let id = self.register_font(source, &bytes);
        self.math_fonts.insert(name.to_owned(), id);
        Ok(id)
    }

    fn math_layout(
        &mut self,
        source: &str,
        em: f32,
        fallback: Option<&Font>,
    ) -> Result<(Vec<Placement>, ratex_types::display_item::DisplayList), String> {
        use ratex_types::display_item::DisplayItem;
        let display = super::math_text::layout(source)?;
        let mut glyphs = Vec::new();
        for item in &display.items {
            if let DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code,
                color,
            } = item
            {
                let font = self.math_font(font, fallback)?;
                let face = ttf_parser::Face::parse(&self.fonts[&font], 0)
                    .map_err(|error| error.to_string())?;
                let ch = char::from_u32(*char_code).ok_or("公式包含无效字符")?;
                let glyph = face
                    .glyph_index(ch)
                    .ok_or_else(|| format!("公式字体不含字符：{ch}"))?
                    .0;
                let key = GlyphKey {
                    font,
                    face: 0,
                    glyph,
                };
                if let std::collections::hash_map::Entry::Vacant(entry) = self.glyphs.entry(key) {
                    entry.insert(GlyphStatus::Pending);
                    self.queue.push_back(key);
                }
                glyphs.push(Placement {
                    key,
                    baseline: Vec2::new(*x as f32, *y as f32) * em,
                    size: *scale as f32 * em,
                    color: (color.r >= 0.0).then(|| super::math_text::ink(*color, Color::WHITE)),
                });
            }
        }
        Ok((glyphs, display))
    }

    fn solid_material(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<MsdfMaterial>,
    ) -> Handle<MsdfMaterial> {
        self.solid
            .get_or_insert_with(|| {
                let image = images.add(Image::new_fill(
                    Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    &[255; 4],
                    TextureFormat::Rgba8Unorm,
                    RenderAssetUsages::all(),
                ));
                materials.add(MsdfMaterial {
                    atlas: image,
                    pixel_range: 1.0,
                })
            })
            .clone()
    }
    fn register_font(&mut self, source_id: u64, bytes: &[u8]) -> u64 {
        if let Some(&id) = self.font_aliases.get(&source_id) {
            return id;
        }
        // Fontique blob IDs can change when an identical font is loaded again.
        let id = self
            .fonts
            .iter()
            .find_map(|(&id, data)| (data.as_ref() == bytes).then_some(id))
            .unwrap_or(source_id);
        self.fonts.entry(id).or_insert_with(|| Arc::from(bytes));
        self.font_aliases.insert(source_id, id);
        id
    }

    pub fn status(&self) -> MsdfStatus {
        self.status
    }

    pub fn pending(&self) -> usize {
        self.queue.len() + self.tasks.iter().map(|(count, _)| count).sum::<usize>()
    }

    fn insert(
        &mut self,
        glyph: GlyphBitmap,
        images: &mut Assets<Image>,
        materials: &mut Assets<MsdfMaterial>,
    ) -> Result<CachedGlyph, String> {
        let source = Image::new(
            Extent3d {
                width: glyph.width,
                height: glyph.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            glyph.rgba,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::all(),
        );
        for index in 0..=self.pages.len() {
            if index == self.pages.len() {
                if index == MAX_PAGES {
                    return Err("MSDF 字形缓存已达 64 MiB 上限，请保存后重新打开文档。".into());
                }
                let mut image = Image::new_fill(
                    Extent3d {
                        width: ATLAS_SIZE,
                        height: ATLAS_SIZE,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    &[0, 0, 0, 255],
                    TextureFormat::Rgba8Unorm,
                    RenderAssetUsages::all(),
                );
                image.sampler = ImageSampler::linear();
                let image = images.add(image);
                self.pages.push(AtlasPage {
                    material: materials.add(MsdfMaterial {
                        atlas: image.clone(),
                        pixel_range: PIXEL_RANGE,
                    }),
                    image,
                    layout: TextureAtlasLayout::new_empty(UVec2::splat(ATLAS_SIZE)),
                    builder: DynamicTextureAtlasBuilder::new(UVec2::splat(ATLAS_SIZE), 1),
                });
            }
            let page = &mut self.pages[index];
            let mut image = images
                .get_mut(&page.image)
                .ok_or("MSDF atlas image is missing")?;
            match page.builder.add_texture(
                &mut page.layout,
                &source,
                image.bypass_change_detection(),
            ) {
                Ok(slot) => {
                    let _ = image.into_inner();
                    return Ok(CachedGlyph {
                        page: index,
                        rect: page.layout.textures[slot],
                        plane: glyph.plane,
                    });
                }
                Err(DynamicTextureAtlasBuilderError::FailedToAllocateSpace) => (),
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("MSDF glyph does not fit in an atlas page".into())
    }

    fn poll(&mut self, images: &mut Assets<Image>, materials: &mut Assets<MsdfMaterial>) {
        let mut i = 0;
        while i < self.tasks.len() {
            if let Some(results) = check_ready(&mut self.tasks[i].1) {
                let _ = self.tasks.swap_remove(i);
                for (key, result) in results {
                    let status = match result {
                        Ok(GeneratedGlyph::Color) => GlyphStatus::Color,
                        Ok(GeneratedGlyph::Outline(bitmap)) => match bitmap
                            .map(|bitmap| self.insert(bitmap, images, materials))
                            .transpose()
                        {
                            Ok(glyph) => {
                                self.generated += 1;
                                GlyphStatus::Ready(glyph)
                            }
                            Err(error) => GlyphStatus::Failed(error),
                        },
                        Err(error) => GlyphStatus::Failed(error),
                    };
                    self.glyphs.insert(key, status);
                }
            } else {
                i += 1;
            }
        }
        while self.tasks.len() < MAX_TASKS {
            let count = self.queue.len().min(GLYPHS_PER_TASK);
            if count == 0 {
                break;
            }
            let batch: Vec<_> = self
                .queue
                .drain(..count)
                .map(|key| (key, self.fonts[&key.font].clone()))
                .collect();
            self.tasks.push((
                count,
                AsyncComputeTaskPool::get().spawn(async move {
                    batch
                        .into_iter()
                        .map(|(key, font)| {
                            (
                                key,
                                kayak_font_msdf::generate_for_rendering(&font, key.face, key.glyph),
                            )
                        })
                        .collect()
                }),
            ));
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct MsdfMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub(super) atlas: Handle<Image>,
    #[uniform(2)]
    pub(super) pixel_range: f32,
}

impl Material2d for MsdfMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://project_graph_bevy_canvas/msdf_text.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

pub fn install(app: &mut App) {
    embedded_asset!(app, "msdf_text.wgsl");
    app.init_resource::<MsdfCache>()
        .add_plugins(Material2dPlugin::<MsdfMaterial>::default());
}

#[derive(Default)]
struct MeshData {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshData {
    fn push(&mut self, placement: &Placement, glyph: CachedGlyph, offset: Vec2) {
        let [left, bottom, right, top] = glyph.plane.map(|v| v * placement.size);
        let origin = Vec2::new(placement.baseline.x, -placement.baseline.y) + offset;
        let base = self.positions.len() as u32;
        self.positions.extend([
            [origin.x + left, origin.y + bottom, 0.0],
            [origin.x + right, origin.y + bottom, 0.0],
            [origin.x + right, origin.y + top, 0.0],
            [origin.x + left, origin.y + top, 0.0],
        ]);
        let lo = glyph.rect.min.as_vec2() / ATLAS_SIZE as f32;
        let hi = glyph.rect.max.as_vec2() / ATLAS_SIZE as f32;
        self.uvs
            .extend([[lo.x, hi.y], [hi.x, hi.y], [hi.x, lo.y], [lo.x, lo.y]]);
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn mesh(self, color: Color) -> Mesh {
        let colors = vec![color.to_linear().to_f32_array(); self.positions.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update(
    mut commands: Commands,
    mut cache: ResMut<MsdfCache>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<MsdfMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    fonts: Res<Assets<Font>>,
    mut pipeline: ResMut<TextPipeline>,
    mut font_cx: ResMut<FontCx>,
    mut layout_cx: ResMut<LayoutCx>,
    mut editor: ResMut<super::editor::Editor>,
    mut labels: Query<(
        Entity,
        Ref<MsdfText>,
        Ref<TextFont>,
        Ref<TextLayout>,
        Ref<TextBounds>,
        Ref<LineHeight>,
        Ref<LetterSpacing>,
        Ref<Anchor>,
        Ref<TextColor>,
        &mut ComputedTextBlock,
        &mut TextLayoutInfo,
        &mut TextState,
        Option<&super::math_text::MathText>,
    )>,
) {
    if !cache.fallbacks_initialized || fonts.is_changed() {
        configure_symbol_fallbacks(&mut font_cx);
        cache.fallbacks_initialized = true;
    }
    cache.poll(&mut images, &mut materials);
    for (
        entity,
        text,
        font,
        layout,
        bounds,
        line_height,
        spacing,
        anchor,
        color,
        mut computed,
        mut info,
        mut state,
        math,
    ) in &mut labels
    {
        let needs_layout = !state.initialized
            || text.is_changed()
            || font.is_changed()
            || layout.is_changed()
            || bounds.is_changed()
            || line_height.is_changed()
            || spacing.is_changed();
        if needs_layout {
            if let bevy::text::FontSource::Handle(handle) = &font.font
                && !fonts.contains(handle.id())
            {
                continue;
            }
            if math.is_some() {
                state.initialized = true;
                state.failed = false;
                state.color_fallback = false;
                state.dirty = true;
                state.glyphs.clear();
                state.math = None;
                let fallback = if let bevy::text::FontSource::Handle(handle) = &font.font {
                    fonts.get(handle)
                } else {
                    None
                };
                let em = font.font_size.eval(Vec2::splat(1000.0), 16.0);
                match cache.math_layout(&text.0, em, fallback) {
                    Ok((glyphs, display)) => {
                        info.size =
                            Vec2::new(display.width as f32, display.total_height() as f32) * em;
                        info.scale_factor = 1.0;
                        state.glyphs = glyphs;
                        state.math = Some(display);
                    }
                    Err(error) => {
                        state.failed = true;
                        editor.report_error(format!("LaTeX 公式排版失败：{error}"));
                    }
                }
            } else {
                let result = pipeline.update_buffer(
                    &fonts,
                    std::iter::once((
                        entity,
                        0,
                        text.0.as_str(),
                        &*font,
                        color.0,
                        *line_height,
                        *spacing,
                    )),
                    layout.linebreak,
                    layout.justify,
                    *bounds,
                    1.0,
                    &mut computed,
                    &mut font_cx,
                    &mut layout_cx,
                    Vec2::splat(1000.0),
                    16.0,
                );
                state.initialized = true;
                state.failed = false;
                state.color_fallback = false;
                state.dirty = true;
                state.glyphs.clear();
                if let Err(error) = result {
                    editor.report_error(format!("MSDF 排版失败：{error}"));
                    state.failed = true;
                    continue;
                }
                let buffer = computed.buffer();
                info.size = Vec2::new(buffer.full_width(), buffer.height());
                info.scale_factor = 1.0;
                let mut missing_glyph = false;
                for line in buffer.lines() {
                    for item in line.items() {
                        let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                            continue;
                        };
                        let run = glyph_run.run();
                        let data = run.font();
                        if run.normalized_coords().iter().any(|&coord| coord != 0) {
                            editor.report_error(
                                "MSDF 当前不支持非默认可变字体轴，请使用静态字体。".into(),
                            );
                            state.failed = true;
                            continue;
                        }
                        let font_id = cache.register_font(data.data.id(), data.data.as_ref());
                        for glyph in glyph_run.positioned_glyphs() {
                            let Ok(id) = u16::try_from(glyph.id) else {
                                editor.report_error("MSDF 字形编号超出 OpenType 范围。".into());
                                state.failed = true;
                                continue;
                            };
                            if id == 0 {
                                missing_glyph = true;
                                continue;
                            }
                            let key = GlyphKey {
                                font: font_id,
                                face: data.index,
                                glyph: id,
                            };
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                cache.glyphs.entry(key)
                            {
                                entry.insert(GlyphStatus::Pending);
                                cache.queue.push_back(key);
                            }
                            state.glyphs.push(Placement {
                                key,
                                baseline: Vec2::new(glyph.x, glyph.y),
                                size: run.font_size(),
                                color: None,
                            });
                        }
                    }
                }
                if missing_glyph {
                    editor.report_error(format!(
                        "系统字体无法显示部分字符，请安装包含这些字符的字体后重启。文本：{}",
                        text.0.chars().take(80).collect::<String>()
                    ));
                    state.failed = true;
                }
            }
        }
        state.dirty |= anchor.is_changed() || color.is_changed();
        if !state.dirty || state.failed || state.color_fallback {
            continue;
        }
        let mut ready = true;
        let mut failure = None;
        let mut color_fallback = false;
        for glyph in &state.glyphs {
            match cache.glyphs.get(&glyph.key) {
                Some(GlyphStatus::Ready(_)) => (),
                Some(GlyphStatus::Color) => color_fallback = true,
                Some(GlyphStatus::Failed(error)) => {
                    failure = Some(error.clone());
                    break;
                }
                _ => ready = false,
            }
        }
        if let Some(error) = failure {
            editor.report_error(format!("MSDF 字形生成失败：{error}"));
            state.failed = true;
            continue;
        }
        if color_fallback {
            state.color_fallback = true;
            continue;
        }
        if !ready {
            continue;
        }
        let size = if math.is_some() {
            info.size
        } else {
            Vec2::new(
                bounds.width.unwrap_or(info.size.x),
                bounds.height.unwrap_or(info.size.y),
            )
        };
        let offset = (Anchor::TOP_LEFT.as_vec() - anchor.as_vec()) * size;
        let mut pages: BTreeMap<(usize, [u32; 4]), MeshData> = BTreeMap::new();
        for placement in &state.glyphs {
            if let Some(GlyphStatus::Ready(Some(glyph))) = cache.glyphs.get(&placement.key) {
                pages
                    .entry((
                        glyph.page,
                        placement
                            .color
                            .unwrap_or(color.0)
                            .to_linear()
                            .to_f32_array()
                            .map(f32::to_bits),
                    ))
                    .or_default()
                    .push(placement, *glyph, offset);
            }
        }
        for part in state.parts.drain(..) {
            commands.entity(part).despawn();
        }
        for ((page, rgba), data) in pages {
            let [r, g, b, a] = rgba.map(f32::from_bits);
            let part = commands
                .spawn((
                    Mesh2d(meshes.add(data.mesh(Color::linear_rgba(r, g, b, a)))),
                    MeshMaterial2d(cache.pages[page].material.clone()),
                    Transform::default(),
                    ChildOf(entity),
                ))
                .id();
            state.parts.push(part);
        }
        if let Some(display) = &state.math {
            match super::math_text::geometry(
                display,
                font.font_size.eval(Vec2::splat(1000.0), 16.0),
                offset,
                color.0,
            ) {
                Ok(Some(mesh)) => {
                    let material = cache.solid_material(&mut images, &mut materials);
                    let part = commands
                        .spawn((
                            Mesh2d(meshes.add(mesh)),
                            MeshMaterial2d(material),
                            // Color boxes sit behind glyphs; rules and radicals
                            // remain visible through their uncovered regions.
                            Transform::from_xyz(0.0, 0.0, -0.001),
                            ChildOf(entity),
                        ))
                        .id();
                    state.parts.push(part);
                }
                Ok(None) => {}
                Err(error) => {
                    state.failed = true;
                    editor.report_error(error);
                }
            }
        }
        state.dirty = state.failed;
        cache.status.mesh_rebuilds += 1;
    }
    cache.status.pending_glyphs = cache.pending();
    cache.status.generated_glyphs = cache.generated;
    cache.status.atlas_pages = cache.pages.len();
    cache.status.atlas_bytes = cache.pages.len() * ATLAS_SIZE as usize * ATLAS_SIZE as usize * 4;
    cache.status.pending_labels = 0;
    cache.status.failed_labels = 0;
    cache.status.color_labels = 0;
    for (
        entity,
        text,
        font,
        layout,
        bounds,
        line_height,
        spacing,
        anchor,
        color,
        _,
        _,
        mut state,
        _,
    ) in &mut labels
    {
        if state.failed || state.color_fallback {
            cache.status.failed_labels += usize::from(state.failed);
            cache.status.color_labels += usize::from(state.color_fallback);
            if state.dirty {
                for part in state.parts.drain(..) {
                    commands.entity(part).despawn();
                }
                let part = commands
                    .spawn((
                        Text2d::new(&text.0),
                        (*font).clone(),
                        *layout,
                        *bounds,
                        *line_height,
                        *spacing,
                        *anchor,
                        *color,
                        Transform::default(),
                        ChildOf(entity),
                    ))
                    .id();
                state.parts.push(part);
                state.dirty = false;
            }
        } else if !state.initialized || state.dirty {
            cache.status.pending_labels += 1;
        }
    }
}
