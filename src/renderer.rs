//! A small GLES2-compatible GPU painter. Rust owns vector glyphs and the draw list;
//! SDL only supplies the context. No browser, LVGL or Qt runtime.
use crate::{
    font::Font,
    raster::{Size, Viewport},
    state::Rect,
    vector,
};
use gl::types::*;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

pub type Color = [f32; 4];
pub const WHITE: Color = [0.96, 0.96, 0.96, 1.];
const GLYPH_GEOMETRY_BUDGET: usize = 8 * 1024 * 1024;
const TEXT_GEOMETRY_BUDGET: usize = 32 * 1024 * 1024;
const TEXTURE_BUDGET: usize = 48 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub size: f32,
    pub weight: u16,
    /// CSS letter-spacing in em, not pixels.
    pub tracking: f32,
    pub line_height: f32,
    pub align_right: bool,
}
impl TextStyle {
    pub const fn new(size: f32) -> Self {
        Self {
            size,
            weight: 400,
            tracking: 0.,
            line_height: 1.2,
            align_right: false,
        }
    }
    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
    pub const fn tracking(mut self, tracking: f32) -> Self {
        self.tracking = tracking;
        self
    }
    pub const fn leading(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }
    pub const fn right(mut self) -> Self {
        self.align_right = true;
        self
    }
}

#[derive(Clone)]
pub enum Draw {
    Shape(Rect, f32, f32, Color),
    RoundedGradient(Rect, f32, Color, Color),
    Shadow(Rect, f32, f32, Color),
    Text(String, Rect, TextStyle, Color, bool),
    Image(String, Rect, f32, bool),
    TintedImage(String, Rect, f32, bool, Color),
    AppImage(Arc<crate::app_artwork::Artwork>, Rect, Color),
    Clip(Option<Rect>),
    EdgeFade(Option<[f32; 4]>),
}

struct Texture {
    id: GLuint,
    width: u32,
    height: u32,
    frame: u64,
}
impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
        }
    }
}

struct Glyph {
    geometry: Vec<luma_vector_geometry::Vertex>,
    frame: u64,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    weight: u16,
    // Logical size, tracking, line height, available width and height.
    layout: [u32; 5],
    flags: u8,
}

struct TextRun {
    mesh: Option<vector::Mesh>,
    missing_glyphs: usize,
    failures: usize,
    frame: u64,
}

#[derive(Clone, Serialize)]
pub struct GraphicsInfo {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    pub max_texture_size: u32,
    pub max_viewport: [i32; 2],
    pub fragment_highp: bool,
}

#[derive(Clone, Serialize)]
pub struct RenderStats {
    pub target: Size,
    pub viewport: Viewport,
    pub font_antialiasing: &'static str,
    pub multisample_samples: u32,
    pub max_logical_font_px: f32,
    pub max_physical_font_px: f32,
    pub cached_glyphs: usize,
    pub glyph_tessellations: usize,
    pub glyph_failures: usize,
    pub missing_glyphs: usize,
    pub missing_images: usize,
    pub glyph_geometry_bytes: usize,
    pub cached_text_runs: usize,
    pub text_geometry_bytes: usize,
    pub text_triangles: usize,
    pub artwork_bytes: usize,
    pub vector_meshes: usize,
    pub vector_bytes: usize,
    pub vector_triangles: usize,
    pub high_resolution_wallpaper: bool,
    pub draw_calls: usize,
    pub cpu_submission_ms: f64,
}
impl RenderStats {
    fn new(target: Size) -> Self {
        Self {
            target,
            viewport: Viewport::new(target),
            font_antialiasing: "vector grayscale coverage",
            multisample_samples: 0,
            max_logical_font_px: 0.,
            max_physical_font_px: 0.,
            cached_glyphs: 0,
            glyph_tessellations: 0,
            glyph_failures: 0,
            missing_glyphs: 0,
            missing_images: 0,
            glyph_geometry_bytes: 0,
            cached_text_runs: 0,
            text_geometry_bytes: 0,
            text_triangles: 0,
            artwork_bytes: 0,
            vector_meshes: 0,
            vector_bytes: 0,
            vector_triangles: 0,
            high_resolution_wallpaper: false,
            draw_calls: 0,
            cpu_submission_ms: 0.,
        }
    }
}

enum BatchKind {
    Paint {
        texture: GLuint,
        start: usize,
        count: usize,
    },
    Vector {
        path: String,
        rect: Rect,
        tint: Color,
    },
    Text {
        key: TextKey,
        origin: [f32; 2],
        tint: Color,
    },
}

struct Batch {
    kind: BatchKind,
    clip: Option<Rect>,
    fade: Option<[f32; 4]>,
}

pub struct Renderer {
    program: GLuint,
    vector_program: GLuint,
    text_program: GLuint,
    buffer: GLuint,
    vao: GLuint,
    white: Texture,
    font_directory: PathBuf,
    fonts: HashMap<u16, Font>,
    failed_fonts: HashSet<u16>,
    glyphs: HashMap<(char, u16), Glyph>,
    text_runs: HashMap<TextKey, TextRun>,
    textures: HashMap<String, Option<Texture>>,
    vectors: HashMap<String, Option<vector::Mesh>>,
    artwork_variants: HashMap<String, String>,
    vertices: Vec<f32>,
    batches: Vec<Batch>,
    clip: Option<Rect>,
    fade: Option<[f32; 4]>,
    frame: u64,
    stats: RenderStats,
    pub info: GraphicsInfo,
}

impl Renderer {
    pub fn new(font: &Path, gles: bool) -> Result<Self, String> {
        let directory = font.parent().ok_or("Invalid font path")?;
        let mut fonts = HashMap::new();
        // Open only the common face. Metrics and outlines stay lazy even when
        // the much larger CJK fallback is needed later.
        fonts.insert(400, load_font(directory, 400)?);
        let mut max_texture_size = 0;
        let mut max_viewport = [0; 2];
        let mut precision = 0;
        unsafe {
            gl::GetIntegerv(gl::MAX_TEXTURE_SIZE, &mut max_texture_size);
            gl::GetIntegerv(gl::MAX_VIEWPORT_DIMS, max_viewport.as_mut_ptr());
            if gles && gl::GetShaderPrecisionFormat::is_loaded() {
                let mut range = [0; 2];
                gl::GetShaderPrecisionFormat(
                    gl::FRAGMENT_SHADER,
                    gl::HIGH_FLOAT,
                    range.as_mut_ptr(),
                    &mut precision,
                );
            }
        }
        let fragment_highp = !gles || precision > 0;
        let qualifier = if fragment_highp { "highp" } else { "mediump" };
        let vertex_prefix = if gles {
            format!("#version 100\nprecision highp float;\n#define PAINT_PRECISION {qualifier}\n")
        } else {
            "#version 150\n#define attribute in\n#define varying out\n#define PAINT_PRECISION\n"
                .into()
        };
        let fragment_prefix = if gles {
            format!(
                "#version 100\nprecision {qualifier} float;\n#define PAINT_PRECISION {qualifier}\n"
            )
        } else {
            "#version 150\n#define varying in\n#define texture2D texture\nout vec4 out_color;\n#define gl_FragColor out_color\n#define PAINT_PRECISION\n".into()
        };
        let program = link(
            &(vertex_prefix.clone() + include_str!("shaders/paint.vert")),
            &(fragment_prefix.clone() + include_str!("shaders/paint.frag")),
            &[c"a_position", c"a_uv", c"a_color", c"a_shape", c"a_params"],
        )?;
        let vector_program = match link(
            &(vertex_prefix.clone() + include_str!("shaders/vector.vert")),
            &(fragment_prefix.clone() + include_str!("shaders/vector.frag")),
            &[c"a_position", c"a_offset", c"a_color", c"a_coverage"],
        ) {
            Ok(program) => program,
            Err(error) => {
                unsafe { gl::DeleteProgram(program) };
                return Err(error);
            }
        };
        let text_program = match link(
            &(vertex_prefix + include_str!("shaders/text.vert")),
            &(fragment_prefix + include_str!("shaders/vector.frag")),
            &[c"a_position", c"a_offset", c"a_origin", c"a_coverage"],
        ) {
            Ok(program) => program,
            Err(error) => {
                unsafe {
                    gl::DeleteProgram(program);
                    gl::DeleteProgram(vector_program);
                }
                return Err(error);
            }
        };
        let mut buffer = 0;
        let mut vao = 0;
        unsafe {
            if !gles {
                gl::GenVertexArrays(1, &mut vao);
                gl::BindVertexArray(vao);
            }
            gl::GenBuffers(1, &mut buffer);
            gl::Enable(gl::BLEND);
            // Preserve an opaque destination alpha. Using BlendFunc for both
            // channels makes translucent UI darken again in PNG/window compositing.
            gl::BlendFuncSeparate(
                gl::SRC_ALPHA,
                gl::ONE_MINUS_SRC_ALPHA,
                gl::ONE,
                gl::ONE_MINUS_SRC_ALPHA,
            );
            gl::Disable(gl::DEPTH_TEST);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        }
        let white = texture(1, 1, &[255; 4]);
        Ok(Self {
            program,
            vector_program,
            text_program,
            buffer,
            vao,
            white,
            font_directory: directory.into(),
            fonts,
            failed_fonts: HashSet::new(),
            glyphs: HashMap::new(),
            text_runs: HashMap::new(),
            textures: HashMap::new(),
            vectors: HashMap::new(),
            artwork_variants: HashMap::new(),
            vertices: Vec::with_capacity(65536),
            batches: Vec::new(),
            clip: None,
            fade: None,
            frame: 0,
            stats: RenderStats::new(Size::HD),
            info: GraphicsInfo {
                vendor: gl_string(gl::VENDOR),
                renderer: gl_string(gl::RENDERER),
                version: gl_string(gl::VERSION),
                max_texture_size: max_texture_size.max(0) as u32,
                max_viewport,
                fragment_highp,
            },
        })
    }

    fn ensure_font(&mut self, id: u16) -> bool {
        if self.fonts.contains_key(&id) {
            return true;
        }
        if self.failed_fonts.contains(&id) {
            return false;
        }
        match load_font(&self.font_directory, id) {
            Ok(font) => {
                self.fonts.insert(id, font);
                true
            }
            Err(error) => {
                eprintln!("{error}");
                self.failed_fonts.insert(id);
                false
            }
        }
    }

    fn glyph(&mut self, c: char, font: u16) -> Result<&[luma_vector_geometry::Vertex], String> {
        let key = (c, font);
        if !self.glyphs.contains_key(&key) {
            let geometry = self.fonts[&font].geometry(c)?;
            self.glyphs.insert(
                key,
                Glyph {
                    geometry,
                    frame: self.frame,
                },
            );
            self.stats.glyph_tessellations += 1;
        }
        let glyph = self.glyphs.get_mut(&key).unwrap();
        glyph.frame = self.frame;
        Ok(&glyph.geometry)
    }

    fn trim_text_geometry(&mut self) {
        let glyph_bytes =
            |glyph: &Glyph| glyph.geometry.len() * size_of::<luma_vector_geometry::Vertex>();
        let mut bytes: usize = self.glyphs.values().map(glyph_bytes).sum();
        if bytes > GLYPH_GEOMETRY_BUDGET || self.glyphs.len() > 4096 {
            let mut old: Vec<_> = self
                .glyphs
                .iter()
                .filter(|(_, glyph)| glyph.frame != self.frame)
                .map(|(key, glyph)| (*key, glyph.frame))
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (key, _) in old {
                if bytes <= GLYPH_GEOMETRY_BUDGET && self.glyphs.len() <= 4096 {
                    break;
                }
                if let Some(glyph) = self.glyphs.remove(&key) {
                    bytes -= glyph_bytes(&glyph);
                }
            }
        }
        let run_bytes = |run: &TextRun| run.mesh.as_ref().map_or(0, |mesh| mesh.bytes);
        let mut bytes: usize = self.text_runs.values().map(run_bytes).sum();
        if bytes > TEXT_GEOMETRY_BUDGET || self.text_runs.len() > 512 {
            let mut old: Vec<_> = self
                .text_runs
                .iter()
                .filter(|(_, run)| run.frame != self.frame)
                .map(|(key, run)| (key.clone(), run.frame))
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (key, _) in old {
                if bytes <= TEXT_GEOMETRY_BUDGET && self.text_runs.len() <= 512 {
                    break;
                }
                if let Some(run) = self.text_runs.remove(&key) {
                    bytes -= run_bytes(&run);
                }
            }
        }
        self.stats.cached_glyphs = self.glyphs.len();
        self.stats.glyph_geometry_bytes = self.glyphs.values().map(glyph_bytes).sum();
        self.stats.cached_text_runs = self.text_runs.len();
        self.stats.text_geometry_bytes = self.text_runs.values().map(run_bytes).sum();
    }

    fn quad(
        &mut self,
        rect: Rect,
        uv: [f32; 4],
        radius: f32,
        border: f32,
        color: Color,
        texture: GLuint,
    ) {
        self.gradient_quad(rect, uv, radius, border, [color, color], texture);
    }

    fn gradient_quad(
        &mut self,
        rect: Rect,
        uv: [f32; 4],
        radius: f32,
        border: f32,
        colors: [Color; 2],
        texture: GLuint,
    ) {
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        let start = self.vertices.len() / 14;
        for (x, y) in [(0., 0.), (1., 0.), (0., 1.), (0., 1.), (1., 0.), (1., 1.)] {
            let color = colors[y as usize];
            self.vertices.extend_from_slice(&[
                rect.x + x * rect.w,
                rect.y + y * rect.h,
                uv[0] + x * (uv[2] - uv[0]),
                uv[1] + y * (uv[3] - uv[1]),
                color[0],
                color[1],
                color[2],
                color[3],
                (x - 0.5) * rect.w,
                (y - 0.5) * rect.h,
                rect.w / 2.,
                rect.h / 2.,
                radius,
                border,
            ]);
        }
        if let Some(batch) = self.batches.last_mut() {
            let same_clip = match (batch.clip, self.clip) {
                (None, None) => true,
                (Some(a), Some(b)) => a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h,
                _ => false,
            };
            if same_clip
                && batch.fade == self.fade
                && let BatchKind::Paint {
                    texture: previous,
                    count,
                    ..
                } = &mut batch.kind
                && *previous == texture
            {
                *count += 6;
                return;
            }
        }
        self.batches.push(Batch {
            kind: BatchKind::Paint {
                texture,
                start,
                count: 6,
            },
            clip: self.clip,
            fade: self.fade,
        });
    }

    fn text(&mut self, text: &str, rect: Rect, style: TextStyle, color: Color, centered: bool) {
        // Layout remains logical. Reuse the same run geometry at every density;
        // its shader snaps insertion origins and computes pixel-sized coverage.
        let size_f = style.size;
        let viewport = self.stats.viewport;
        self.stats.max_logical_font_px = self.stats.max_logical_font_px.max(size_f);
        self.stats.max_physical_font_px = self
            .stats
            .max_physical_font_px
            .max(size_f * viewport.text_scale());
        let padded = rect.inset(-size_f);
        if padded.x + padded.w < 0.
            || padded.y + padded.h < 0.
            || padded.x > 1920.
            || padded.y > 1080.
            || self.clip.is_some_and(|clip| {
                padded.x + padded.w < clip.x
                    || padded.y + padded.h < clip.y
                    || padded.x > clip.x + clip.w
                    || padded.y > clip.y + clip.h
            })
        {
            return;
        }
        let origin = [rect.x, rect.y];
        let key = TextKey {
            text: text.into(),
            weight: style.weight,
            layout: [
                size_f.to_bits(),
                style.tracking.to_bits(),
                style.line_height.to_bits(),
                rect.w.to_bits(),
                rect.h.to_bits(),
            ],
            flags: u8::from(style.align_right)
                | (u8::from(centered) << 1)
                | (u8::from(self.stats.multisample_samples > 1) << 2),
        };
        if self.text_runs.contains_key(&key) {
            self.submit_text(key, origin, color);
            return;
        }
        let rect = Rect::new(0., 0., rect.w, rect.h);
        let font_id = style.weight + if style.size >= 32. { 1000 } else { 0 };
        let cjk_id = if style.weight >= 500 { 2500 } else { 2400 };
        let hangul_id = cjk_id + 1000;
        if !self.ensure_font(font_id) {
            self.stats.glyph_failures += text.chars().count();
            return;
        }
        let (mut needs_cjk, mut needs_hangul) = (false, false);
        for c in text.chars() {
            if !c.is_ascii_control() && self.fonts[&font_id].lookup_glyph_index(c) == 0 {
                if is_hangul(c) {
                    needs_hangul = true;
                } else {
                    needs_cjk = true;
                }
            }
        }
        // Each fallback loads only when a run needs it, so Korean text never
        // pulls in the larger Chinese font and vice versa.
        let cjk_ready = needs_cjk && self.ensure_font(cjk_id);
        let hangul_ready = needs_hangul && self.ensure_font(hangul_id);
        let fonts = &self.fonts;
        let choose_font = |c: char| {
            if fonts[&font_id].lookup_glyph_index(c) != 0 {
                font_id
            } else if is_hangul(c) {
                if hangul_ready { hangul_id } else { font_id }
            } else if cjk_ready {
                cjk_id
            } else {
                font_id
            }
        };
        let font = &fonts[&font_id];
        let line_height = size_f * style.line_height;
        let baseline = font.horizontal_line_metrics(size_f).map_or(size_f, |m| {
            (line_height - m.ascent + m.descent) / 2. + m.ascent
        });
        let tracking = style.tracking * size_f;
        let advance = |c| {
            if c == '\n' {
                0.
            } else {
                fonts[&choose_font(c)].metrics(c, size_f).advance_width + tracking
            }
        };
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut width = 0.;
        // Word wrapping, with character fallback for a title wider than the tile.
        for word in text.split_inclusive(char::is_whitespace) {
            let word_width: f32 = word.chars().map(advance).sum();
            if width > 0. && width + word_width > rect.w {
                lines.push(line.trim_end().into());
                line.clear();
                width = 0.;
            }
            for c in word.chars() {
                if c == '\n' {
                    lines.push(line.trim_end().into());
                    line.clear();
                    width = 0.;
                    continue;
                }
                let w = advance(c);
                if width > 0. && width + w > rect.w {
                    lines.push(line.trim_end().into());
                    line.clear();
                    width = 0.;
                }
                line.push(c);
                width += w;
            }
        }
        if !line.is_empty() {
            lines.push(line.trim_end().into());
        }
        let mut placed = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let top = rect.y + index as f32 * line_height;
            if index > 0 && top + line_height > rect.y + rect.h + 1. {
                break;
            }
            let mut previous = None;
            let width: f32 = line
                .chars()
                .map(|c| {
                    let kern = previous
                        .and_then(|p| font.horizontal_kern(p, c, size_f))
                        .unwrap_or(0.);
                    previous = Some(c);
                    advance(c) + kern
                })
                .sum();
            let mut x = rect.x
                + if style.align_right {
                    rect.w - width
                } else if centered {
                    (rect.w - width) / 2.
                } else {
                    0.
                };
            let mut previous = None;
            for c in line.chars() {
                if let Some(previous) = previous {
                    x += font.horizontal_kern(previous, c, size_f).unwrap_or(0.);
                }
                let selected = choose_font(c);
                let baseline = if selected == font_id {
                    baseline
                } else {
                    fonts[&selected]
                        .horizontal_line_metrics(size_f)
                        .map_or(baseline, |m| {
                            (line_height - m.ascent + m.descent) / 2. + m.ascent
                        })
                };
                placed.push((c, x, top + baseline, selected));
                x += advance(c);
                previous = Some(c);
            }
        }
        let mut vertices = Vec::new();
        let mut missing_glyphs = 0;
        let mut failures = 0;
        let multisampled = self.stats.multisample_samples > 1;
        for (c, x, baseline, selected) in placed {
            if self.fonts[&selected].lookup_glyph_index(c) == 0 && !c.is_whitespace() {
                missing_glyphs += 1;
            }
            match self.glyph(c, selected) {
                Ok(geometry) => {
                    for triangle in geometry.chunks_exact(3) {
                        // True sample coverage needs only the original fill.
                        // Exclude the boundary fringe, not just its alpha.
                        if multisampled && triangle.iter().any(|vertex| vertex.coverage < 1.) {
                            continue;
                        }
                        for vertex in triangle {
                            vertices.extend_from_slice(&[
                                vertex.position.x * size_f,
                                vertex.position.y * size_f,
                                if multisampled { 0. } else { vertex.offset[0] },
                                if multisampled { 0. } else { vertex.offset[1] },
                                x,
                                baseline,
                                0.,
                                0.,
                                vertex.coverage,
                            ]);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Luma glyph {c:?}: {error}");
                    failures += 1;
                }
            }
        }
        let mesh = if vertices.is_empty() {
            None
        } else {
            match vector::Mesh::upload(1., 1., &vertices) {
                Ok(mesh) => Some(mesh),
                Err(error) => {
                    eprintln!("Luma text: {error}");
                    failures += 1;
                    None
                }
            }
        };
        self.text_runs.insert(
            key.clone(),
            TextRun {
                mesh,
                missing_glyphs,
                failures,
                frame: self.frame,
            },
        );
        self.submit_text(key, origin, color);
    }

    fn submit_text(&mut self, key: TextKey, origin: [f32; 2], tint: Color) {
        let run = self.text_runs.get_mut(&key).unwrap();
        run.frame = self.frame;
        self.stats.missing_glyphs += run.missing_glyphs;
        self.stats.glyph_failures += run.failures;
        if let Some(mesh) = &run.mesh {
            self.stats.text_triangles += mesh.vertices as usize / 3;
            self.batches.push(Batch {
                kind: BatchKind::Text { key, origin, tint },
                clip: self.clip,
                fade: self.fade,
            });
        }
    }

    fn image(&mut self, path: &str, mut rect: Rect, radius: f32, contain: bool, tint: Color) {
        if path.ends_with(".lvec") {
            self.vector_image(path, rect, contain, tint);
            return;
        }
        let path = if self.stats.viewport.scale > 1.
            && self.info.max_texture_size >= 3840
            && Path::new(path)
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|p| p == "wallpapers")
        {
            self.artwork_variants
                .entry(path.into())
                .or_insert_with(|| {
                    let original = Path::new(path);
                    let variant = original.with_file_name(format!(
                        "{}@2x.jpg",
                        original.file_stem().unwrap_or_default().to_string_lossy()
                    ));
                    if variant.is_file() {
                        variant.to_string_lossy().into_owned()
                    } else {
                        path.into()
                    }
                })
                .clone()
        } else {
            path.into()
        };
        if path.ends_with("@2x.jpg") {
            self.stats.high_resolution_wallpaper = true;
        }
        let frame = self.frame;
        let entry = self
            .textures
            .entry(path.clone())
            .or_insert_with(|| match image::open(&path) {
                Ok(image) => {
                    let image = image.into_rgba8();
                    Some(texture(image.width(), image.height(), image.as_raw()))
                }
                Err(error) => {
                    eprintln!("Artwork {path}: {error}");
                    None
                }
            });
        let Some(texture) = entry else {
            self.stats.missing_images += 1;
            return;
        };
        texture.frame = frame;
        let id = texture.id;
        if contain {
            let scale = (rect.w / texture.width as f32).min(rect.h / texture.height as f32);
            let w = texture.width as f32 * scale;
            let h = texture.height as f32 * scale;
            rect = Rect::new(rect.x + (rect.w - w) / 2., rect.y + (rect.h - h) / 2., w, h);
        }
        self.quad(rect, [0., 0., 1., 1.], radius, 0., tint, id);
    }

    fn vector_image(&mut self, path: &str, mut rect: Rect, contain: bool, tint: Color) {
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        let mesh = self.vectors.entry(path.into()).or_insert_with(|| {
            match vector::Mesh::load(Path::new(path)) {
                Ok(mesh) => Some(mesh),
                Err(error) => {
                    eprintln!("{error}");
                    None
                }
            }
        });
        let Some(mesh) = mesh else {
            self.stats.missing_images += 1;
            return;
        };
        if contain {
            let scale = (rect.w / mesh.width).min(rect.h / mesh.height);
            let width = mesh.width * scale;
            let height = mesh.height * scale;
            rect = Rect::new(
                rect.x + (rect.w - width) / 2.,
                rect.y + (rect.h - height) / 2.,
                width,
                height,
            );
        }
        let clip = if let Some(clip) = self.clip {
            let x = rect.x.max(clip.x);
            let y = rect.y.max(clip.y);
            Rect::new(
                x,
                y,
                (rect.x + rect.w).min(clip.x + clip.w).max(x) - x,
                (rect.y + rect.h).min(clip.y + clip.h).max(y) - y,
            )
        } else {
            rect
        };
        if clip.w <= 0. || clip.h <= 0. {
            return;
        }
        self.stats.vector_triangles += mesh.vertices as usize / 3;
        self.batches.push(Batch {
            kind: BatchKind::Vector {
                path: path.into(),
                rect,
                tint,
            },
            clip: Some(clip),
            fade: self.fade,
        });
    }

    fn app_image(&mut self, image: &crate::app_artwork::Artwork, rect: Rect, tint: Color) {
        let entry = self.textures.entry(image.key.clone()).or_insert_with(|| {
            Some(texture(
                image.pixels.width(),
                image.pixels.height(),
                image.pixels.as_raw(),
            ))
        });
        let Some(texture) = entry else {
            return;
        };
        texture.frame = self.frame;
        let scale = (rect.w / texture.width as f32).min(rect.h / texture.height as f32);
        let w = texture.width as f32 * scale;
        let h = texture.height as f32 * scale;
        let id = texture.id;
        self.quad(
            Rect::new(rect.x + (rect.w - w) / 2., rect.y + (rect.h - h) / 2., w, h),
            [0., 0., 1., 1.],
            0.,
            0.,
            tint,
            id,
        );
    }

    pub fn render(&mut self, commands: &[Draw], width: u32, height: u32) {
        let started = Instant::now();
        self.stats = RenderStats::new(Size::new(width, height));
        // Captures can bind a single-sample FBO even when the actual TV window
        // is multisampled, so query the active target rather than assume.
        let mut samples = 0;
        unsafe { gl::GetIntegerv(gl::SAMPLES, &mut samples) };
        self.stats.multisample_samples = samples.max(0) as u32;
        if samples > 1 {
            self.stats.font_antialiasing = "multisample vector coverage";
        }
        self.frame += 1;
        self.vertices.clear();
        self.batches.clear();
        self.clip = None;
        self.fade = None;
        for command in commands {
            match command {
                Draw::Shape(rect, radius, border, color) => self.quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    *border,
                    *color,
                    self.white.id,
                ),
                Draw::RoundedGradient(rect, radius, top, bottom) => self.gradient_quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    0.,
                    [*top, *bottom],
                    self.white.id,
                ),
                Draw::Shadow(rect, radius, blur, color) => self.quad(
                    rect.inset(-*blur),
                    [0., 0., 1., 1.],
                    *radius,
                    -*blur,
                    *color,
                    self.white.id,
                ),
                Draw::Text(text, rect, size, color, centered) => {
                    self.text(text, *rect, *size, *color, *centered)
                }
                Draw::Image(path, rect, radius, contain) => {
                    self.image(path, *rect, *radius, *contain, [1.; 4])
                }
                Draw::TintedImage(path, rect, radius, contain, tint) => {
                    self.image(path, *rect, *radius, *contain, *tint)
                }
                Draw::AppImage(image, rect, tint) => self.app_image(image, *rect, *tint),
                Draw::Clip(clip) => self.clip = *clip,
                Draw::EdgeFade(fade) => self.fade = *fade,
            }
        }
        let Viewport {
            x: vx,
            y: vy,
            width: vw,
            height: vh,
            scale,
        } = self.stats.viewport;
        unsafe {
            gl::Disable(gl::SCISSOR_TEST);
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::ClearColor(0.04, 0.04, 0.05, 1.);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::Viewport(vx, vy, vw, vh);
            if self.vao != 0 {
                gl::BindVertexArray(self.vao);
            }
            gl::BindBuffer(gl::ARRAY_BUFFER, self.buffer);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.vertices.len() * size_of::<f32>()) as isize,
                self.vertices.as_ptr().cast(),
                gl::DYNAMIC_DRAW,
            );
            gl::UseProgram(self.program);
            gl::Uniform1f(
                gl::GetUniformLocation(self.program, c"u_aa".as_ptr()),
                0.85 / scale.max(0.01),
            );
            gl::Uniform1i(
                gl::GetUniformLocation(self.program, c"u_texture".as_ptr()),
                0,
            );
            gl::ActiveTexture(gl::TEXTURE0);
            let mut paint_bound = false;
            let mut current_fade = None;
            for batch in &self.batches {
                if current_fade != Some(batch.fade) {
                    // The same cheap alpha mask applies to shapes, textures,
                    // icons and text; the unmasked wallpaper remains beneath.
                    let fade = batch.fade.unwrap_or([0., 0., 0., 0.]);
                    for program in [self.program, self.vector_program, self.text_program] {
                        gl::UseProgram(program);
                        gl::Uniform4f(
                            gl::GetUniformLocation(program, c"u_edge_fade".as_ptr()),
                            fade[0],
                            fade[1],
                            fade[2],
                            fade[3],
                        );
                    }
                    current_fade = Some(batch.fade);
                    paint_bound = false;
                }
                if let Some(clip) = batch.clip {
                    gl::Enable(gl::SCISSOR_TEST);
                    gl::Scissor(
                        vx + (clip.x * vw as f32 / 1920.).round() as i32,
                        vy + ((1080. - clip.y - clip.h) * vh as f32 / 1080.).round() as i32,
                        (clip.w * vw as f32 / 1920.).round() as i32,
                        (clip.h * vh as f32 / 1080.).round() as i32,
                    );
                } else {
                    gl::Disable(gl::SCISSOR_TEST);
                }
                match &batch.kind {
                    BatchKind::Paint {
                        texture,
                        start,
                        count,
                    } => {
                        if !paint_bound {
                            gl::UseProgram(self.program);
                            gl::BindBuffer(gl::ARRAY_BUFFER, self.buffer);
                            let mut offset = 0;
                            for (index, size) in [2, 2, 4, 4, 2].into_iter().enumerate() {
                                gl::EnableVertexAttribArray(index as u32);
                                gl::VertexAttribPointer(
                                    index as u32,
                                    size,
                                    gl::FLOAT,
                                    gl::FALSE,
                                    56,
                                    (offset * size_of::<f32>()) as *const _,
                                );
                                offset += size as usize;
                            }
                            paint_bound = true;
                        }
                        gl::BindTexture(gl::TEXTURE_2D, *texture);
                        gl::DrawArrays(gl::TRIANGLES, *start as i32, *count as i32);
                    }
                    BatchKind::Vector { path, rect, tint } => {
                        let Some(mesh) = self.vectors.get(path).and_then(Option::as_ref) else {
                            continue;
                        };
                        self.draw_mesh(
                            self.vector_program,
                            mesh,
                            [rect.w / mesh.width, rect.h / mesh.height, rect.x, rect.y],
                            *tint,
                        );
                        paint_bound = false;
                    }
                    BatchKind::Text { key, origin, tint } => {
                        let Some(mesh) = self.text_runs.get(key).and_then(|run| run.mesh.as_ref())
                        else {
                            continue;
                        };
                        self.draw_mesh(
                            self.text_program,
                            mesh,
                            [1., 1., origin[0], origin[1]],
                            *tint,
                        );
                        paint_bound = false;
                    }
                }
            }
            gl::Disable(gl::SCISSOR_TEST);
        }
        // Keep the current scene, evict old full-resolution wallpapers before memory grows.
        let mut bytes: usize = self
            .textures
            .values()
            .flatten()
            .map(|t| t.width as usize * t.height as usize * 4)
            .sum();
        if bytes > TEXTURE_BUDGET {
            let mut old: Vec<_> = self
                .textures
                .iter()
                .filter_map(|(path, t)| {
                    t.as_ref()
                        .filter(|t| t.frame != self.frame)
                        .map(|t| (path.clone(), t.frame))
                })
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (path, _) in old {
                if bytes <= TEXTURE_BUDGET {
                    break;
                }
                if let Some(Some(t)) = self.textures.remove(&path) {
                    bytes -= t.width as usize * t.height as usize * 4;
                }
            }
        }
        self.trim_text_geometry();
        self.stats.vector_meshes = self.vectors.values().flatten().count();
        self.stats.vector_bytes = self.vectors.values().flatten().map(|mesh| mesh.bytes).sum();
        self.stats.artwork_bytes = bytes + self.stats.vector_bytes;
        self.stats.draw_calls = self.batches.len();
        self.stats.cpu_submission_ms = started.elapsed().as_secs_f64() * 1000.;
    }

    fn draw_mesh(&self, program: GLuint, mesh: &vector::Mesh, transform: [f32; 4], tint: Color) {
        let viewport = self.stats.viewport;
        unsafe {
            gl::UseProgram(program);
            if program == self.text_program {
                gl::Uniform1f(
                    gl::GetUniformLocation(program, c"u_fringe".as_ptr()),
                    if self.stats.multisample_samples > 1 {
                        0.
                    } else {
                        0.5
                    },
                );
            }
            gl::BindBuffer(gl::ARRAY_BUFFER, mesh.buffer);
            gl::DisableVertexAttribArray(4);
            let mut offset = 0;
            for (index, size) in [2, 2, 4, 1].into_iter().enumerate() {
                gl::EnableVertexAttribArray(index as u32);
                gl::VertexAttribPointer(
                    index as u32,
                    size,
                    gl::FLOAT,
                    gl::FALSE,
                    36,
                    (offset * size_of::<f32>()) as *const _,
                );
                offset += size as usize;
            }
            gl::Uniform4f(
                gl::GetUniformLocation(program, c"u_transform".as_ptr()),
                transform[0],
                transform[1],
                transform[2],
                transform[3],
            );
            gl::Uniform2f(
                gl::GetUniformLocation(program, c"u_pixel_scale".as_ptr()),
                viewport.width as f32 / 1920.,
                viewport.height as f32 / 1080.,
            );
            gl::Uniform4f(
                gl::GetUniformLocation(program, c"u_tint".as_ptr()),
                tint[0],
                tint[1],
                tint[2],
                tint[3],
            );
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertices);
        }
    }

    pub fn stats(&self) -> &RenderStats {
        &self.stats
    }

    pub fn screenshot(&self, path: &Path, width: u32, height: u32) -> Result<(), String> {
        let mut pixels = vec![0; width as usize * height as usize * 4];
        unsafe {
            gl::ReadPixels(
                0,
                0,
                width as i32,
                height as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
        }
        let mut image = image::RgbaImage::from_raw(width, height, pixels)
            .ok_or("Invalid framebuffer dimensions")?;
        image::imageops::flip_vertical_in_place(&mut image);
        image.save(path).map_err(|e| e.to_string())
    }

    /// Capture the authored TV surface, independent of desktop HiDPI/window limits.
    pub fn capture(
        &mut self,
        commands: &[Draw],
        path: &Path,
        size: Size,
    ) -> Result<RenderStats, String> {
        if size.width > self.info.max_texture_size
            || size.height > self.info.max_texture_size
            || size.width > self.info.max_viewport[0] as u32
            || size.height > self.info.max_viewport[1] as u32
        {
            return Err(format!(
                "Capture {}x{} exceeds GPU limits: {}px texture, {:?} viewport",
                size.width, size.height, self.info.max_texture_size, self.info.max_viewport
            ));
        }
        let image = texture(size.width, size.height, &[]);
        let mut framebuffer = 0;
        unsafe {
            gl::GenFramebuffers(1, &mut framebuffer);
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                image.id,
                0,
            );
            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::DeleteFramebuffers(1, &framebuffer);
                return Err("Screenshot framebuffer is incomplete".into());
            }
        }
        self.render(commands, size.width, size.height);
        let stats = self.stats.clone();
        let result = self.screenshot(path, size.width, size.height);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteFramebuffers(1, &framebuffer);
        }
        result.map(|_| stats)
    }
}

// Noto Sans SC (font-cjk) has no Hangul syllables; these go to font-kr instead.
// Keep in sync with is_hangul() in tools/font-weights.py, which builds font-kr.
fn is_hangul(c: char) -> bool {
    matches!(
        c,
        '\u{1100}'..='\u{11FF}'
            | '\u{3130}'..='\u{318F}'
            | '\u{A960}'..='\u{A97F}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{D7B0}'..='\u{D7FF}'
    )
}

fn load_font(directory: &Path, id: u16) -> Result<Font, String> {
    let name = if id >= 3000 {
        format!("font-kr-{}.ttf", id - 3000)
    } else if id >= 2000 {
        format!("font-cjk-{}.ttf", id - 2000)
    } else if id >= 1000 {
        format!("font-display-{}.ttf", id - 1000)
    } else {
        format!("font-{id}.ttf")
    };
    let path = directory.join(&name);
    let started = Instant::now();
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Font {}: {e}. Regenerate native assets.", path.display()))?;
    let font = Font::from_bytes(bytes).map_err(|e| format!("Font {}: {e}", path.display()))?;
    if cfg!(feature = "webos") {
        eprintln!(
            "Luma font: {name} loaded in {} ms",
            started.elapsed().as_millis()
        );
    }
    Ok(font)
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.buffer);
            gl::DeleteProgram(self.program);
            gl::DeleteProgram(self.vector_program);
            gl::DeleteProgram(self.text_program);
            if self.vao != 0 {
                gl::DeleteVertexArrays(1, &self.vao);
            }
        }
    }
}

fn texture(width: u32, height: u32, pixels: &[u8]) -> Texture {
    let mut id = 0;
    unsafe {
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            width as i32,
            height as i32,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            if pixels.is_empty() {
                std::ptr::null()
            } else {
                pixels.as_ptr().cast()
            },
        );
    }
    Texture {
        id,
        width,
        height,
        frame: 0,
    }
}

fn gl_string(name: GLenum) -> String {
    unsafe {
        let value = gl::GetString(name);
        if value.is_null() {
            "unknown".into()
        } else {
            CStr::from_ptr(value.cast()).to_string_lossy().into_owned()
        }
    }
}

fn compile(kind: GLenum, source: &str) -> Result<GLuint, String> {
    let source = CString::new(source).map_err(|e| e.to_string())?;
    unsafe {
        let shader = gl::CreateShader(kind);
        gl::ShaderSource(shader, 1, &source.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);
        let mut success = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
        if success == 0 {
            let mut log = vec![0u8; 4096];
            let mut len = 0;
            gl::GetShaderInfoLog(shader, log.len() as i32, &mut len, log.as_mut_ptr().cast());
            gl::DeleteShader(shader);
            return Err(format!(
                "Shader compilation: {}",
                String::from_utf8_lossy(&log[..len as usize])
            ));
        }
        Ok(shader)
    }
}

fn link(vertex: &str, fragment: &str, attributes: &[&CStr]) -> Result<GLuint, String> {
    let vertex = compile(gl::VERTEX_SHADER, vertex)?;
    let fragment = match compile(gl::FRAGMENT_SHADER, fragment) {
        Ok(fragment) => fragment,
        Err(error) => {
            unsafe {
                gl::DeleteShader(vertex);
            }
            return Err(error);
        }
    };
    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vertex);
        gl::AttachShader(program, fragment);
        for (index, name) in attributes.iter().enumerate() {
            gl::BindAttribLocation(program, index as u32, name.as_ptr());
        }
        gl::LinkProgram(program);
        gl::DeleteShader(vertex);
        gl::DeleteShader(fragment);
        let mut success = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
        if success == 0 {
            let mut log = vec![0u8; 4096];
            let mut len = 0;
            gl::GetProgramInfoLog(program, log.len() as i32, &mut len, log.as_mut_ptr().cast());
            gl::DeleteProgram(program);
            return Err(format!(
                "Shader linking: {}",
                String::from_utf8_lossy(&log[..len as usize])
            ));
        }
        Ok(program)
    }
}
