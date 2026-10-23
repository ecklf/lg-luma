//! Host-only SVG compiler. No XML parser or tessellator is linked into the TV app.
use luma_vector_geometry as geometry;
use luma_vector_geometry::lyon_tessellation::{
    FillRule,
    math::{Point, point},
    path::Path,
};
use serde::Deserialize;
use std::{fs, io::Write, path::PathBuf};
use usvg::{
    Paint,
    tiny_skia_path::{PathSegment, Transform},
};

const MAGIC: &[u8; 8] = b"LUMAVEC1";
const MAX_VERTICES: usize = 1_000_000;

#[derive(Deserialize)]
struct Asset {
    source: PathBuf,
    output: PathBuf,
    #[serde(default)]
    monochrome: bool,
    #[serde(default)]
    light_ink: bool,
    #[serde(default)]
    trim: bool,
    reference_size: f32,
}

#[derive(Clone, Copy)]
struct Vertex {
    position: Point,
    offset: [f32; 2],
    coverage: f32,
}

impl Vertex {
    fn midpoint(self, other: Self) -> Self {
        Self {
            position: (self.position + other.position.to_vector()) * 0.5,
            offset: [
                (self.offset[0] + other.offset[0]) * 0.5,
                (self.offset[1] + other.offset[1]) * 0.5,
            ],
            coverage: (self.coverage + other.coverage) * 0.5,
        }
    }
}

struct Painter<'a> {
    paint: &'a Paint,
    inverse: Transform,
    opacity: f32,
    monochrome: bool,
    light_ink: bool,
}

impl Painter<'_> {
    fn color(&self, position: Point) -> Result<[f32; 4], String> {
        let mut p = usvg::tiny_skia_path::Point::from_xy(position.x, position.y);
        self.inverse.map_point(&mut p);
        let (base, stops, t): (&usvg::BaseGradient, &[usvg::Stop], f32) = match self.paint {
            Paint::Color(color) => {
                return Ok(self.finish(*color, self.opacity));
            }
            Paint::LinearGradient(gradient) => {
                gradient
                    .transform()
                    .invert()
                    .ok_or("Singular SVG gradient transform")?
                    .map_point(&mut p);
                let dx = gradient.x2() - gradient.x1();
                let dy = gradient.y2() - gradient.y1();
                let length = dx * dx + dy * dy;
                let t = if length > 0. {
                    ((p.x - gradient.x1()) * dx + (p.y - gradient.y1()) * dy) / length
                } else {
                    0.
                };
                (&**gradient, gradient.stops(), t)
            }
            Paint::RadialGradient(gradient) => {
                gradient
                    .transform()
                    .invert()
                    .ok_or("Singular SVG gradient transform")?
                    .map_point(&mut p);
                let dx = p.x - gradient.fx();
                let dy = p.y - gradient.fy();
                let fx = gradient.fx() - gradient.cx();
                let fy = gradient.fy() - gradient.cy();
                let a = dx * dx + dy * dy;
                let b = 2. * (fx * dx + fy * dy);
                let c = fx * fx + fy * fy - gradient.r().get().powi(2);
                let discriminant = (b * b - 4. * a * c).max(0.);
                let lambda = if a > 0. {
                    (-b + discriminant.sqrt()) / (2. * a)
                } else {
                    0.
                };
                let t = if lambda > 0. { 1. / lambda } else { 0. };
                (&**gradient, gradient.stops(), t)
            }
            Paint::Pattern(_) => {
                return Err("SVG patterns are not supported by vector artwork".into());
            }
        };
        let t = match base.spread_method() {
            usvg::SpreadMethod::Pad => t.clamp(0., 1.),
            usvg::SpreadMethod::Repeat => t.rem_euclid(1.),
            usvg::SpreadMethod::Reflect => {
                let t = t.rem_euclid(2.);
                if t > 1. { 2. - t } else { t }
            }
        };
        let first = stops.first().ok_or("SVG gradient has no stops")?;
        let last = stops.last().unwrap();
        if t <= first.offset().get() {
            return Ok(self.finish(first.color(), self.opacity * first.opacity().get()));
        }
        for pair in stops.windows(2) {
            if t <= pair[1].offset().get() {
                let a = self.finish(pair[0].color(), self.opacity * pair[0].opacity().get());
                let b = self.finish(pair[1].color(), self.opacity * pair[1].opacity().get());
                let span = pair[1].offset().get() - pair[0].offset().get();
                let t = if span > 0. {
                    (t - pair[0].offset().get()) / span
                } else {
                    1.
                };
                return Ok(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t));
            }
        }
        Ok(self.finish(last.color(), self.opacity * last.opacity().get()))
    }

    fn finish(&self, color: usvg::Color, opacity: f32) -> [f32; 4] {
        if self.monochrome || (self.light_ink && color.red.max(color.green).max(color.blue) < 80) {
            [1., 1., 1., opacity]
        } else {
            [
                color.red as f32 / 255.,
                color.green as f32 / 255.,
                color.blue as f32 / 255.,
                opacity,
            ]
        }
    }
}

struct Compiler {
    vertices: Vec<[f32; 9]>,
    tolerance: f32,
    reference_scale: f32,
    monochrome: bool,
    light_ink: bool,
}

impl Compiler {
    fn group(&mut self, group: &usvg::Group) -> Result<(), String> {
        if group.clip_path().is_some() || group.mask().is_some() || !group.filters().is_empty() {
            return Err("Clips, masks and filters are not supported by vector artwork".into());
        }
        if group.opacity().get() != 1. {
            return Err(
                "Group opacity needs isolated compositing; use path opacity in vector artwork"
                    .into(),
            );
        }
        for node in group.children() {
            match node {
                usvg::Node::Group(group) => self.group(group)?,
                usvg::Node::Path(path) if path.is_visible() => match path.paint_order() {
                    usvg::PaintOrder::FillAndStroke => {
                        self.fill(path)?;
                        self.stroke(path)?;
                    }
                    usvg::PaintOrder::StrokeAndFill => {
                        self.stroke(path)?;
                        self.fill(path)?;
                    }
                },
                usvg::Node::Path(_) => {}
                _ => {
                    return Err(
                        "Vector artwork must contain paths, not embedded images or text".into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn fill(&mut self, path: &usvg::Path) -> Result<(), String> {
        if let Some(fill) = path.fill() {
            let rule = match fill.rule() {
                usvg::FillRule::NonZero => FillRule::NonZero,
                usvg::FillRule::EvenOdd => FillRule::EvenOdd,
            };
            self.path(
                path.data(),
                path.abs_transform(),
                fill.paint(),
                fill.opacity().get(),
                rule,
            )?;
        }
        Ok(())
    }

    fn stroke(&mut self, path: &usvg::Path) -> Result<(), String> {
        if let Some(stroke) = path.stroke() {
            let transform = path.abs_transform();
            let scale = self.reference_scale
                * (transform.sx.hypot(transform.ky)).max(transform.kx.hypot(transform.sy));
            let style = stroke.to_tiny_skia();
            let data = match &style.dash {
                Some(dash) => path
                    .data()
                    .dash(dash, scale)
                    .ok_or("Could not resolve dashed SVG stroke")?,
                None => path.data().clone(),
            };
            if let Some(outline) = data.stroke(&style, scale) {
                self.path(
                    &outline,
                    transform,
                    stroke.paint(),
                    stroke.opacity().get(),
                    FillRule::NonZero,
                )?;
            }
        }
        Ok(())
    }

    fn path(
        &mut self,
        data: &usvg::tiny_skia_path::Path,
        transform: Transform,
        paint: &Paint,
        opacity: f32,
        rule: FillRule,
    ) -> Result<(), String> {
        let painter = Painter {
            paint,
            inverse: transform.invert().ok_or("Singular SVG path transform")?,
            opacity,
            monochrome: self.monochrome,
            light_ink: self.light_ink,
        };
        let data = data
            .clone()
            .transform(transform)
            .ok_or("Invalid SVG path transform")?;
        let mut builder = Path::builder();
        let mut open = false;
        for segment in data.segments() {
            match segment {
                PathSegment::MoveTo(p) => {
                    if open {
                        builder.end(true);
                    }
                    builder.begin(point(p.x, p.y));
                    open = true;
                }
                PathSegment::LineTo(p) => {
                    builder.line_to(point(p.x, p.y));
                }
                PathSegment::QuadTo(a, b) => {
                    builder.quadratic_bezier_to(point(a.x, a.y), point(b.x, b.y));
                }
                PathSegment::CubicTo(a, b, c) => {
                    builder.cubic_bezier_to(point(a.x, a.y), point(b.x, b.y), point(c.x, c.y));
                }
                PathSegment::Close => {
                    if open {
                        builder.end(true);
                        open = false;
                    }
                }
            }
        }
        if open {
            builder.end(true);
        }
        let path = builder.build();
        for triangle in geometry::tessellate(&path, self.tolerance, rule)?.chunks_exact(3) {
            self.triangle(
                std::array::from_fn(|index| Vertex {
                    position: triangle[index].position,
                    offset: triangle[index].offset,
                    coverage: triangle[index].coverage,
                }),
                &painter,
                0,
            )?;
        }
        Ok(())
    }

    fn triangle(
        &mut self,
        vertices: [Vertex; 3],
        painter: &Painter,
        depth: usize,
    ) -> Result<(), String> {
        let colors = [
            painter.color(vertices[0].position)?,
            painter.color(vertices[1].position)?,
            painter.color(vertices[2].position)?,
        ];
        if !matches!(painter.paint, Paint::Color(_)) && depth < 8 {
            let midpoints = [
                vertices[0].midpoint(vertices[1]),
                vertices[1].midpoint(vertices[2]),
                vertices[2].midpoint(vertices[0]),
            ];
            let mut error = 0f32;
            let center = point(
                vertices.iter().map(|vertex| vertex.position.x).sum::<f32>() / 3.,
                vertices.iter().map(|vertex| vertex.position.y).sum::<f32>() / 3.,
            );
            let actual = painter.color(center)?;
            for channel in 0..4 {
                error = error.max(
                    (actual[channel] - colors.iter().map(|color| color[channel]).sum::<f32>() / 3.)
                        .abs(),
                );
            }
            for (i, midpoint) in midpoints.iter().enumerate() {
                let actual = painter.color(midpoint.position)?;
                for channel in 0..4 {
                    error = error.max(
                        (actual[channel]
                            - (colors[i][channel] + colors[(i + 1) % 3][channel]) * 0.5)
                            .abs(),
                    );
                }
            }
            if error > 1. / 512. {
                for triangle in [
                    [vertices[0], midpoints[0], midpoints[2]],
                    [midpoints[0], vertices[1], midpoints[1]],
                    [midpoints[2], midpoints[1], vertices[2]],
                    [midpoints[0], midpoints[1], midpoints[2]],
                ] {
                    self.triangle(triangle, painter, depth + 1)?;
                }
                return Ok(());
            }
        }
        if self.vertices.len() + 3 > MAX_VERTICES {
            return Err("Vector artwork exceeds its geometry budget".into());
        }
        for (vertex, color) in vertices.into_iter().zip(colors) {
            self.vertices.push([
                vertex.position.x,
                vertex.position.y,
                vertex.offset[0],
                vertex.offset[1],
                color[0],
                color[1],
                color[2],
                color[3],
                vertex.coverage,
            ]);
        }
        Ok(())
    }
}

fn run() -> Result<(), String> {
    let manifest = std::env::args()
        .nth(1)
        .ok_or("Pass the vector artwork manifest")?;
    let assets: Vec<Asset> =
        serde_json::from_slice(&fs::read(manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut total = 0;
    for asset in &assets {
        let data =
            fs::read(&asset.source).map_err(|e| format!("{}: {e}", asset.source.display()))?;
        let tree = usvg::Tree::from_data(&data, &usvg::Options::default())
            .map_err(|e| format!("{}: {e}", asset.source.display()))?;
        let size = tree.size();
        let scale = asset.reference_size / size.width().max(size.height());
        let mut compiler = Compiler {
            vertices: Vec::new(),
            tolerance: 0.15 / scale,
            reference_scale: scale,
            monochrome: asset.monochrome,
            light_ink: asset.light_ink,
        };
        compiler
            .group(tree.root())
            .map_err(|e| format!("{}: {e}", asset.source.display()))?;
        if compiler.vertices.is_empty() {
            return Err(format!("No vector geometry in {}", asset.source.display()));
        }
        let (width, height) = if asset.trim {
            let bounds = compiler.vertices.iter().fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |b, v| {
                    [
                        b[0].min(v[0]),
                        b[1].min(v[1]),
                        b[2].max(v[0]),
                        b[3].max(v[1]),
                    ]
                },
            );
            for vertex in &mut compiler.vertices {
                vertex[0] -= bounds[0];
                vertex[1] -= bounds[1];
            }
            (bounds[2] - bounds[0], bounds[3] - bounds[1])
        } else {
            (size.width(), size.height())
        };
        let mut bytes = Vec::with_capacity(20 + compiler.vertices.len() * 36);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&(compiler.vertices.len() as u32).to_le_bytes());
        for vertex in &compiler.vertices {
            for value in vertex {
                if !value.is_finite() {
                    return Err(format!(
                        "Non-finite vector geometry in {}",
                        asset.source.display()
                    ));
                }
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        fs::create_dir_all(asset.output.parent().ok_or("Invalid vector output path")?)
            .map_err(|e| e.to_string())?;
        let mut file = fs::File::create(&asset.output).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        total += bytes.len();
    }
    println!(
        "Compiled {} SVG assets into cached vector geometry ({:.2} MiB).",
        assets.len(),
        total as f64 / 1048576.
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Vector artwork: {error}");
        std::process::exit(1);
    }
}
