use super::*;
use bevy::{
    camera::{RenderTarget, ScalingMode},
    render::{render_resource::TextureUsages, view::window::screenshot::ScreenshotCaptured},
};

#[derive(Resource)]
pub struct ExportImage {
    path: PathBuf,
    rect: [f32; 4],
    size: [u32; 2],
    transparent: bool,
    target: Option<Handle<Image>>,
    ready_at: Option<Instant>,
    started: Instant,
    requested: bool,
    automatic_bounds: bool,
}

impl ExportImage {
    pub fn from_args(args: &[String], graph: &Graph) -> Result<Option<Self>> {
        let Some(path) = arg(args, "--render-png") else {
            return Ok(None);
        };
        let rect = if let Some(rect) = arg(args, "--export-rect") {
            let values: Vec<f32> = rect
                .split(',')
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            values
                .try_into()
                .map_err(|_| "导出区域应为 x,y,width,height")?
        } else {
            let [x, y, w, h] = graph.bounds();
            [x - 32.0, y - 32.0, w + 64.0, h + 64.0]
        };
        let width: u32 = arg(args, "--export-width")
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(2048);
        let size = dimensions(rect, width)?;
        Ok(Some(Self {
            path: path.into(),
            rect,
            size,
            transparent: args.iter().any(|arg| arg == "--transparent"),
            target: None,
            ready_at: None,
            started: Instant::now(),
            requested: false,
            automatic_bounds: arg(args, "--export-rect").is_none(),
        }))
    }
}

fn dimensions(rect: [f32; 4], width: u32) -> Result<[u32; 2]> {
    if !rect.iter().all(|value| value.is_finite()) || rect[2] <= 0.0 || rect[3] <= 0.0 || width == 0
    {
        return Err("导出区域与分辨率必须为有限正数".into());
    }
    let height = (f64::from(width) * f64::from(rect[3]) / f64::from(rect[2])).ceil();
    if height > 8192.0 || width > 8192 || height < 1.0 || f64::from(width) * height > 16_777_216.0 {
        return Err("导出图片超过限制：单边 8192，总计 16777216 像素".into());
    }
    Ok([width, height as u32])
}

pub fn setup(
    mut commands: Commands,
    mut job: ResMut<ExportImage>,
    mut images: ResMut<Assets<Image>>,
    camera: Single<Entity, With<Camera2d>>,
    config: Res<Config>,
) {
    let mut image = Image::new_fill(
        Extent3d {
            width: job.size[0],
            height: job.size[1],
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::COPY_SRC
        | TextureUsages::RENDER_ATTACHMENT;
    let target = images.add(image);
    commands.entity(*camera).insert((
        RenderTarget::Image(target.clone().into()),
        Camera {
            clear_color: bevy::camera::ClearColorConfig::Custom(if job.transparent {
                Color::NONE
            } else {
                theme::palette(config.light).canvas
            }),
            ..default()
        },
    ));
    job.target = Some(target);
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn render(
    mut commands: Commands,
    mut job: ResMut<ExportImage>,
    cache: Res<msdf_text::MsdfCache>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    mut exit: MessageWriter<AppExit>,
    bounds: Query<
        (
            &GlobalTransform,
            &bevy::camera::primitives::Aabb,
            &InheritedVisibility,
        ),
        Or<(With<Mesh2d>, With<Sprite>, With<Text2d>)>,
    >,
    mut images: ResMut<Assets<Image>>,
) {
    if job.automatic_bounds && !job.requested {
        let rect = rendered_bounds(
            bounds
                .iter()
                .filter(|(_, _, visible)| visible.get())
                .map(|(transform, bounds, _)| (transform, bounds)),
        );
        if let Some(rect) = rect
            && rect.iter().zip(job.rect).any(|(a, b)| (a - b).abs() > 0.01)
        {
            let size = match dimensions(rect, job.size[0]) {
                Ok(size) => size,
                Err(error) => {
                    error!("PNG export bounds invalid: {error}");
                    exit.write(AppExit::error());
                    return;
                }
            };
            if let Some(target) = &job.target
                && let Some(mut image) = images.get_mut(target)
                && job.size != size
            {
                image.resize(Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                });
            }
            job.rect = rect;
            job.size = size;
            job.ready_at = None;
        }
    }
    let [x, y, w, h] = job.rect;
    camera.0.translation = Vec3::new(x + w * 0.5, -y - h * 0.5, 0.0);
    if let Projection::Orthographic(projection) = &mut *camera.1 {
        projection.scale = 1.0;
        projection.scaling_mode = ScalingMode::Fixed {
            width: w,
            height: h,
        };
    }
    if job.started.elapsed().as_secs() > 120 {
        error!("PNG export timed out");
        exit.write(AppExit::error());
        return;
    }
    if job.requested {
        return;
    }
    let status = cache.status();
    if status.failed_labels > 0 {
        error!("PNG export text preparation failed");
        exit.write(AppExit::error());
        return;
    }
    if !status.ready() {
        job.ready_at = None;
        return;
    }
    if job
        .ready_at
        .get_or_insert_with(Instant::now)
        .elapsed()
        .as_secs_f32()
        < 3.0
    {
        return;
    }
    if let Some(target) = &job.target {
        commands
            .spawn(Screenshot::image(target.clone()))
            .observe(save);
        job.requested = true;
    }
}

fn rendered_bounds<'a>(
    bounds: impl Iterator<Item = (&'a GlobalTransform, &'a bevy::camera::primitives::Aabb)>,
) -> Option<[f32; 4]> {
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for (transform, bounds) in bounds {
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let local = Vec3::from(bounds.center)
                        + Vec3::from(bounds.half_extents) * Vec3::new(x, y, z);
                    let world = transform.transform_point(local).truncate();
                    lo = lo.min(world);
                    hi = hi.max(world);
                }
            }
        }
    }
    (lo.is_finite() && hi.is_finite()).then(|| {
        [
            lo.x - 32.0,
            -hi.y - 32.0,
            (hi.x - lo.x).max(1.0) + 64.0,
            (hi.y - lo.y).max(1.0) + 64.0,
        ]
    })
}

fn save(event: On<ScreenshotCaptured>, job: Res<ExportImage>, mut exit: MessageWriter<AppExit>) {
    let result = (|| -> Result<()> {
        let image = event
            .image
            .clone()
            .try_into_dynamic()
            .map_err(|error| format!("无法读取导出像素：{error:?}"))?;
        let mut image = image.into_rgba8();
        if job.transparent {
            straight_alpha(&mut image);
        }
        let parent = job
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        image.write_to(file.as_file_mut(), image::ImageFormat::Png)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(&job.path)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            exit.write(AppExit::Success);
        }
        Err(error) => {
            error!("PNG export failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

fn straight_alpha(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        let alpha = f32::from(pixel[3]) / 255.0;
        if alpha == 0.0 {
            pixel.0 = [0; 4];
        } else if alpha < 1.0 {
            // GPU blending premultiplies in linear light; PNG stores straight sRGB.
            for channel in &mut pixel.0[..3] {
                let srgb = f32::from(*channel) / 255.0;
                let linear = if srgb <= 0.04045 {
                    srgb / 12.92
                } else {
                    ((srgb + 0.055) / 1.055).powf(2.4)
                };
                let linear = (linear / alpha).min(1.0);
                let srgb = if linear <= 0.0031308 {
                    linear * 12.92
                } else {
                    1.055 * linear.powf(1.0 / 2.4) - 0.055
                };
                *channel = (srgb * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendered_bounds_include_transformed_geometry_outside_node_rectangles() {
        let transform = GlobalTransform::from(
            Transform::from_xyz(100.0, -200.0, 0.0).with_scale(Vec3::new(2.0, 3.0, 1.0)),
        );
        let bounds = bevy::camera::primitives::Aabb {
            center: Vec3::new(10.0, 20.0, 0.0).into(),
            half_extents: Vec3::new(30.0, 40.0, 0.0).into(),
        };
        assert_eq!(
            rendered_bounds(std::iter::once((&transform, &bounds))),
            Some([28.0, -12.0, 184.0, 304.0])
        );
        assert!(rendered_bounds(std::iter::empty()).is_none());
    }
    #[test]
    fn transparent_export_unpremultiplies_in_linear_light() {
        let mut image = image::RgbaImage::from_raw(
            3,
            1,
            vec![31, 32, 45, 115, 80, 100, 120, 255, 90, 90, 90, 0],
        )
        .unwrap();
        straight_alpha(&mut image);
        assert_eq!(image.get_pixel(0, 0).0, [49, 50, 68, 115]);
        assert_eq!(image.get_pixel(1, 0).0, [80, 100, 120, 255]);
        assert_eq!(image.get_pixel(2, 0).0, [0, 0, 0, 0]);
    }
    #[test]
    fn export_dimensions_preserve_aspect_and_reject_invalid_allocations() {
        assert_eq!(
            dimensions([-50.0, 100.0, 400.0, 200.0], 1000).unwrap(),
            [1000, 500]
        );
        for (rect, width) in [
            ([0.0, 0.0, 0.0, 1.0], 100),
            ([0.0, 0.0, 1.0, 1.0], 0),
            ([f32::NAN, 0.0, 1.0, 1.0], 100),
            ([0.0, 0.0, 1.0, 100.0], 1000),
            ([0.0, 0.0, 1.0, 1.0], 8192),
        ] {
            assert!(dimensions(rect, width).is_err());
        }
    }
}
