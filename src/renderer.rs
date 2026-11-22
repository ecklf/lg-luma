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
#[cfg(test)]
#[path = "font-size-tests.rs"]
mod font_size_tests;
const GLYPH_GEOMETRY_BUDGET: usize = 8 * 1024 * 1024;
const TEXT_GEOMETRY_BUDGET: usize = 32 * 1024 * 1024;
const TEXT_RUN_LIMIT: usize = 512;
const TEXTURE_BUDGET: usize = 48 * 1024 * 1024;
/// Precomputed shadow coverage: at most this many texels per shadow and in total.
const SHADOW_TEXELS: usize = 1024 * 1024;
const SHADOW_BUDGET: usize = 4 * 1024 * 1024;

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
    /// Shared, downsampled wallpaper backdrop; text/artwork remain separate sharp draws.
    Glass(Rect, f32, Color),
    Shape(Rect, f32, f32, Color),
    RoundedGradient(Rect, f32, Color, Color),
    Shadow(Rect, f32, f32, Color),
    Text(String, Rect, TextStyle, Color, bool),
    Image(String, Rect, f32, bool),
    TintedImage(String, Rect, f32, bool, Color),
    /// Installed icon and its backdrop, composited before opacity and edge fading.
    AppImage(Arc<crate::app_artwork::Artwork>, Rect, f32, Color),
    Clip(Option<Rect>),
    EdgeFade(Option<[f32; 4]>),
}

struct Texture {
    id: GLuint,
    width: u32,
    height: u32,
    frame: u64,
    /// Every texel has alpha 255: an untinted, unfaded draw of it hides what lies beneath.
    opaque: bool,
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
    pub glass_surfaces: usize,
    pub glass_backdrop_sampling: bool,
    pub target: Size,
    pub viewport: Viewport,
    pub font_antialiasing: &'static str,
    pub font_size_snapping: bool,
    pub multisample_samples: u32,
    pub max_logical_font_px: f32,
    pub max_physical_font_px: f32,
    pub cached_glyphs: usize,
    pub glyph_tessellations: usize,
    /// Milliseconds this frame spent on first-use work: glyph outlines, logo meshes, images.
    pub glyph_ms: f64,
    pub vector_load_ms: f64,
    pub image_load_ms: f64,
    pub glass_prepare_ms: f64,
    pub glass_precomputed_loads: usize,
    pub glass_runtime_prepares: usize,
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
    /// Share of the drawable redrawn in the last frame (1 without partial redraw).
    pub redrawn_fraction: f64,
}
impl RenderStats {
    fn new(target: Size) -> Self {
        Self {
            glass_surfaces: 0,
            glass_backdrop_sampling: false,
            target,
            viewport: Viewport::new(target),
            font_antialiasing: "vector grayscale coverage",
            font_size_snapping: std::env::var("LUMA_FONT_SIZE_SNAP").as_deref() != Ok("0"),
            multisample_samples: 0,
            max_logical_font_px: 0.,
            max_physical_font_px: 0.,
            cached_glyphs: 0,
            glyph_tessellations: 0,
            glyph_ms: 0.,
            vector_load_ms: 0.,
            image_load_ms: 0.,
            glass_prepare_ms: 0.,
            glass_precomputed_loads: 0,
            glass_runtime_prepares: 0,
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
            redrawn_fraction: 1.,
        }
    }
}

enum BatchKind {
    Paint {
        texture: GLuint,
        start: usize,
        count: usize,
        backdrop: Option<[[u8; 3]; 2]>,
        /// Drawn without blending: lets the GPU discard hidden fragments beneath.
        opaque: bool,
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

/// Mesh-program state already set within this frame, so unchanged values are not resent.
#[derive(Default)]
struct MeshState {
    /// The mesh program in use (0 after another program was bound).
    program: GLuint,
    /// The vertex array object bound for meshes (None after the paint path bound its own).
    vertex_array: Option<GLuint>,
    vector_scale: Option<[f32; 2]>,
    text_scale: Option<[f32; 2]>,
    fringe: Option<f32>,
    embolden: Option<f32>,
}

/// A region of a quad drawn differently from the rest of it.
#[derive(Clone, Copy)]
enum Hole {
    /// Not drawn: its pixels would not change the target.
    Skip(Rect),
    /// Fully opaque: drawn without blending.
    Opaque(Rect),
}

thread_local! {
    /// Uniform locations by program and name: the driver's lookup is a string search,
    /// and every batch sets a few uniforms. Cleared whenever programs are (re)created.
    static UNIFORMS: std::cell::RefCell<HashMap<(GLuint, usize), GLint>> =
        std::cell::RefCell::new(HashMap::new());
}

/// `name` must be a static C string literal (its address is the cache key).
unsafe fn uniform_location(program: GLuint, name: *const std::ffi::c_char) -> GLint {
    UNIFORMS.with(|cache| {
        *cache
            .borrow_mut()
            .entry((program, name as usize))
            .or_insert_with(|| unsafe { gl::GetUniformLocation(program, name) })
    })
}

fn full_screen(rect: &Rect) -> bool {
    rect.x <= 0. && rect.y <= 0. && rect.x + rect.w >= 1920. && rect.y + rect.h >= 1080.
}

/// An opaque, untinted, square full-screen image: the wallpaper that scrims can fold into.
fn plain_wallpaper(command: &Draw) -> Option<&str> {
    match command {
        Draw::Image(path, rect, radius, false) if full_screen(rect) && *radius <= 0. => Some(path),
        Draw::TintedImage(path, rect, radius, false, tint)
            if full_screen(rect) && *radius <= 0. && *tint == [1.; 4] =>
        {
            Some(path)
        }
        _ => None,
    }
}

/// A full-screen translucent fill without corners or border: a flat colour or a vertical
/// gradient (straight alpha, interpolated like the paint shader's vertex colours).
enum Scrim {
    Flat(Color),
    Gradient(Color, Color),
}
impl Scrim {
    fn of(command: &Draw) -> Option<Self> {
        match command {
            Draw::Shape(rect, radius, border, color)
                if full_screen(rect) && *radius <= 0. && *border <= 0. =>
            {
                Some(Self::Flat(*color))
            }
            Draw::RoundedGradient(rect, radius, top, bottom)
                if full_screen(rect) && *radius <= 0. =>
            {
                Some(Self::Gradient(*top, *bottom))
            }
            _ => None,
        }
    }
    fn at(&self, t: f32) -> Color {
        match self {
            Self::Flat(color) => *color,
            Self::Gradient(top, bottom) => {
                std::array::from_fn(|i| top[i] + (bottom[i] - top[i]) * t)
            }
        }
    }
}

fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let top = (a[1] + a[3]).min(b[1] + b[3]);
    [x, y, (right - x).max(0), (top - y).max(0)]
}

/// Square images skip the rounded-rectangle distance field: a negative radius selects the
/// shader's plain texture path. Per pixel that field cost as much as the rest of the Home scene
/// on a webOS 5 Mali for the full-screen wallpaper.
fn plain_radius(radius: f32) -> f32 {
    if radius > 0. { radius } else { -1. }
}

/// Measurement aid: LUMA_PROFILE_SKIP=wallpaper,image,app,shape,gradient,shadow,text or all
/// leaves those draws out, to find which part of the scene costs the GPU time.
fn profile_skip(command: &Draw) -> bool {
    static SKIP: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let skip = SKIP.get_or_init(|| {
        std::env::var("LUMA_PROFILE_SKIP")
            .map(|value| value.split(',').map(str::to_owned).collect())
            .unwrap_or_default()
    });
    if skip.is_empty() {
        return false;
    }
    let kind = match command {
        Draw::Image(_, rect, ..) | Draw::TintedImage(_, rect, ..)
            if rect.w >= 1920. && rect.h >= 1080. =>
        {
            "wallpaper"
        }
        Draw::Image(..) | Draw::TintedImage(..) => "image",
        Draw::AppImage(..) => "app",
        Draw::Shape(..) => "shape",
        Draw::Glass(..) => "glass",
        Draw::RoundedGradient(..) => "gradient",
        Draw::Shadow(..) => "shadow",
        Draw::Text(..) => "text",
        Draw::Clip(_) | Draw::EdgeFade(_) => return false,
    };
    skip.iter().any(|entry| entry == kind || entry == "all")
}

/// Stem emboldening for small text, in physical pixels per edge: thin stems get an opaque
/// core instead of only grey edge pixels, which a TV's 2x scale-up would soften further.
/// Full strength up to 16 physical pixels, none from 28. Zero in the default MSAA mode.
fn embolden(physical_size: f32, strength: f32) -> f32 {
    strength * ((28. - physical_size) / 12.).clamp(0., 1.)
}

/// Quantize the em size before layout, not individual outline vertices. Round
/// down so text fitted by the view's unsnapped metrics cannot grow and wrap.
/// Rounding in logical pixels would miss fractional drawable densities; rounding
/// only in the shader would leave advances out of step with the visible glyphs.
fn physical_font_size(size: f32, scale: f32, snap: bool) -> f32 {
    if snap {
        (size * scale).floor().max(1.) / scale
    } else {
        size
    }
}

/// Pixel size, corner radius and blur of a cached shadow coverage texture.
pub type ShadowKey = (u32, u32, u32, u32);

fn snap_rect(rect: Rect, scale: f32) -> Rect {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var("LUMA_SNAP").as_deref() != Ok("0")) {
        return rect;
    }
    let scale = scale.max(0.01);
    let snap = |value: f32| (value * scale).round() / scale;
    let (x0, y0) = (snap(rect.x), snap(rect.y));
    // Never collapse a visible hairline.
    let x1 = snap(rect.x + rect.w).max(x0 + 1. / scale);
    let y1 = snap(rect.y + rect.h).max(y0 + 1. / scale);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

struct PaintPrograms {
    texture: GLuint,
    tinted: GLuint,
    shape: GLuint,
    app: GLuint,
}

impl Drop for PaintPrograms {
    fn drop(&mut self) {
        for id in [self.texture, self.tinted, self.shape, self.app] {
            if id != 0 {
                unsafe { gl::DeleteProgram(id) };
            }
        }
    }
}

impl PaintPrograms {
    /// Use a short shader only if every triangle in this batch takes the same paint path.
    fn select(&self, vertices: &[f32], opaque: bool, app: bool, fallback: GLuint) -> GLuint {
        let all = |predicate: fn(&[f32]) -> bool| vertices.chunks_exact(14).all(predicate);
        let inside = opaque && all(|v| v[12] >= 0. && v[13] == 0.);
        if inside && app {
            return self.app;
        }
        if all(|v| v[12] == -1.) || (inside && !app) {
            return if all(|v| v[4..8] == [1.; 4]) {
                self.texture
            } else {
                self.tinted
            };
        }
        if !app && all(|v| v[12] >= 0. && v[13] >= 0.) {
            self.shape
        } else {
            fallback
        }
    }
}

pub struct Renderer {
    paint_programs: Option<PaintPrograms>,
    program: GLuint,
    vector_program: GLuint,
    text_program: GLuint,
    text_embolden: f32,
    buffer: GLuint,
    vao: GLuint,
    /// Whether meshes have their own vertex array objects (see vector::use_vertex_arrays).
    vertex_arrays: bool,
    /// With vertex arrays: the paint attributes over `buffer`, set up once (else 0).
    paint_array: GLuint,
    white: Texture,
    font_directory: PathBuf,
    fonts: HashMap<u16, Font>,
    failed_fonts: HashSet<u16>,
    glyphs: HashMap<(char, u16), Glyph>,
    text_runs: HashMap<TextKey, TextRun>,
    textures: HashMap<String, Option<Texture>>,
    /// Shadow coverage by drawable size, radius and blur (bits), computed once on the CPU.
    shadows: HashMap<(u32, u32, u32, u32), Texture>,
    /// Shadows seen without a texture yet: (quad, radius, blur), prepared while idle.
    pending_shadows: Vec<(Rect, f32, f32)>,
    preparing_shadow: bool,
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
    pub fn new(font: &Path, gles: bool, text_embolden: f32) -> Result<Self, String> {
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
        UNIFORMS.with(|cache| cache.borrow_mut().clear());
        let program = link(
            &(vertex_prefix.clone() + include_str!("shaders/paint.vert")),
            &(fragment_prefix.clone() + include_str!("shaders/paint.frag")),
            &[c"a_position", c"a_uv", c"a_color", c"a_shape", c"a_params"],
        )?;
        let paint_programs = if std::env::var("LUMA_PAINT_SHADERS").as_deref() == Ok("1") {
            let build = |source: &str| {
                link(
                    &(vertex_prefix.clone() + include_str!("shaders/paint.vert")),
                    &(fragment_prefix.clone() + source),
                    &[c"a_position", c"a_uv", c"a_color", c"a_shape", c"a_params"],
                )
            };
            let load = || -> Result<PaintPrograms, String> {
                let mut programs = PaintPrograms {
                    texture: 0,
                    tinted: 0,
                    shape: 0,
                    app: 0,
                };
                programs.texture = build(include_str!("shaders/texture.frag"))?;
                programs.tinted = build(include_str!("shaders/tinted.frag"))?;
                programs.shape = build(include_str!("shaders/shape.frag"))?;
                programs.app = build(include_str!("shaders/app-interior.frag"))?;
                Ok(programs)
            };
            load()
                .inspect_err(|error| eprintln!("Specialized paint shaders unavailable: {error}"))
                .ok()
        } else {
            None
        };
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
            &(fragment_prefix + include_str!("shaders/text.frag")),
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
        let vertex_arrays;
        let mut paint_array = 0;
        unsafe {
            if !gles {
                gl::GenVertexArrays(1, &mut vao);
                gl::BindVertexArray(vao);
            }
            gl::GenBuffers(1, &mut buffer);
            // Meshes record their attribute setup in vertex array objects where the context
            // has them (desktop core, GLES 3): TV drivers charge for every state call.
            vertex_arrays = gl::GenVertexArrays::is_loaded()
                && gl::BindVertexArray::is_loaded()
                && gl::DeleteVertexArrays::is_loaded()
                && (!gles || gl_string(gl::VERSION).starts_with("OpenGL ES 3"))
                && std::env::var("LUMA_VAO").as_deref() != Ok("0");
            vector::use_vertex_arrays(vertex_arrays);
            if vertex_arrays {
                gl::GenVertexArrays(1, &mut paint_array);
                gl::BindVertexArray(paint_array);
                gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
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
                gl::BindVertexArray(vao);
            }
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
            paint_programs,
            vector_program,
            text_program,
            text_embolden,
            buffer,
            vao,
            vertex_arrays,
            paint_array,
            white,
            font_directory: directory.into(),
            fonts,
            failed_fonts: HashSet::new(),
            glyphs: HashMap::new(),
            text_runs: HashMap::new(),
            textures: HashMap::new(),
            shadows: HashMap::new(),
            pending_shadows: Vec::new(),
            preparing_shadow: false,
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
            let started = Instant::now();
            let geometry = self.fonts[&font].geometry(c)?;
            self.stats.glyph_ms += started.elapsed().as_secs_f64() * 1000.;
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

    /// Whether idle preparation may add text runs without evicting ones it prepared earlier:
    /// a quarter of the cache stays free for what the next frames draw.
    pub fn text_cache_has_room(&self) -> bool {
        let bytes: usize = self
            .text_runs
            .values()
            .map(|run| run.mesh.as_ref().map_or(0, |mesh| mesh.bytes))
            .sum();
        self.text_runs.len() < TEXT_RUN_LIMIT * 3 / 4 && bytes < TEXT_GEOMETRY_BUDGET * 3 / 4
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
        if bytes > TEXT_GEOMETRY_BUDGET || self.text_runs.len() > TEXT_RUN_LIMIT {
            let mut old: Vec<_> = self
                .text_runs
                .iter()
                .filter(|(_, run)| run.frame != self.frame)
                .map(|(key, run)| (key.clone(), run.frame))
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (key, _) in old {
                if bytes <= TEXT_GEOMETRY_BUDGET && self.text_runs.len() <= TEXT_RUN_LIMIT {
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
        self.gradient_quad(
            rect,
            uv,
            radius,
            border,
            [color, color],
            texture,
            None,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn gradient_quad(
        &mut self,
        rect: Rect,
        uv: [f32; 4],
        radius: f32,
        border: f32,
        colors: [Color; 2],
        texture: GLuint,
        backdrop: Option<[[u8; 3]; 2]>,
        hole: Option<Hole>,
    ) {
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        let rect = self.snap(rect);
        // Pixels inside `hole` would leave the target unchanged (zero coverage) or are
        // overdrawn by an opaque draw: rasterize only the four strips around it.
        if let Some(region) = hole {
            let (Hole::Skip(hole) | Hole::Opaque(hole)) = region;
            // Whole physical pixels, rounded inward: the strips meet on pixel boundaries.
            let scale = self.stats.viewport.scale.max(0.01);
            let (left, top) = (
                (hole.x * scale).ceil() / scale,
                (hole.y * scale).ceil() / scale,
            );
            let right = ((hole.x + hole.w) * scale).floor() / scale;
            let bottom = ((hole.y + hole.h) * scale).floor() / scale;
            let hole = Rect::new(left, top, right - left, bottom - top);
            let x0 = hole.x.max(rect.x);
            let y0 = hole.y.max(rect.y);
            let x1 = (hole.x + hole.w).min(rect.x + rect.w);
            let y1 = (hole.y + hole.h).min(rect.y + rect.h);
            if x1 - x0 >= 8. && y1 - y0 >= 8. {
                let start = self.vertices.len() / 14;
                let (left, top) = (rect.x, rect.y);
                let (right, bottom) = (rect.x + rect.w, rect.y + rect.h);
                let mut count = 0;
                for [a, b, c, d] in [
                    [left, top, right, y0],
                    [left, y1, right, bottom],
                    [left, y0, x0, y1],
                    [x1, y0, right, y1],
                ] {
                    if c > a && d > b {
                        self.quad_part(rect, [a, b, c, d], uv, radius, border, colors);
                        count += 6;
                    }
                }
                if count > 0 {
                    self.paint_batch(start, count, texture, backdrop, false);
                }
                if let Hole::Opaque(_) = region {
                    let start = self.vertices.len() / 14;
                    self.quad_part(rect, [x0, y0, x1, y1], uv, radius, border, colors);
                    self.paint_batch(start, 6, texture, backdrop, true);
                }
                return;
            }
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
        self.paint_batch(start, 6, texture, backdrop, false);
    }

    /// Six paint vertices for the part [x0, y0, x1, y1] of `rect`, with the same attributes
    /// the whole quad interpolates there.
    fn quad_part(
        &mut self,
        rect: Rect,
        part: [f32; 4],
        uv: [f32; 4],
        radius: f32,
        border: f32,
        colors: [Color; 2],
    ) {
        for (x, y) in [(0, 1), (2, 1), (0, 3), (0, 3), (2, 1), (2, 3)] {
            let (px, py) = (part[x], part[y]);
            let fx = (px - rect.x) / rect.w;
            let fy = (py - rect.y) / rect.h;
            let color: [f32; 4] =
                std::array::from_fn(|i| colors[0][i] + (colors[1][i] - colors[0][i]) * fy);
            self.vertices.extend_from_slice(&[
                px,
                py,
                uv[0] + fx * (uv[2] - uv[0]),
                uv[1] + fy * (uv[3] - uv[1]),
                color[0],
                color[1],
                color[2],
                color[3],
                (fx - 0.5) * rect.w,
                (fy - 0.5) * rect.h,
                rect.w / 2.,
                rect.h / 2.,
                radius,
                border,
            ]);
        }
    }

    /// The region of the next draw that is certainly opaque: an untinted, unfaded app face
    /// or opaque image. A shadow beneath it does not need those pixels.
    fn opaque_cover(&self, next: Option<&Draw>) -> Option<Rect> {
        let (rect, radius) = match next? {
            Draw::AppImage(_, rect, radius, tint) if tint[3] >= 1. => (*rect, *radius),
            Draw::Image(path, rect, radius, false)
                if !path.ends_with(".lvec")
                    && self
                        .textures
                        .get(path.as_str())
                        .is_some_and(|t| t.as_ref().is_some_and(|t| t.opaque)) =>
            {
                (*rect, *radius)
            }
            _ => return None,
        };
        self.opaque_interior(rect, radius)
    }

    /// The opaque region of the later draw that covers `rect`'s centre, before the clip or
    /// edge fade changes: whatever is drawn in between there is overdrawn by it too.
    fn cover_after(&self, later: &[Draw], rect: Rect) -> Option<Rect> {
        let (cx, cy) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
        for command in later.iter().take(512) {
            if matches!(command, Draw::Clip(_) | Draw::EdgeFade(_)) {
                return None;
            }
            if let Some(hole) = self.opaque_cover(Some(command))
                && hole.x <= cx
                && cx <= hole.x + hole.w
                && hole.y <= cy
                && cy <= hole.y + hole.h
            {
                return Some(hole);
            }
        }
        None
    }

    /// The part of an opaque rounded rect drawn with full coverage: inside the rounded
    /// corners, two pixels clear of the antialiased edge and within an edge fade's plateau.
    fn opaque_interior(&self, rect: Rect, radius: f32) -> Option<Rect> {
        let mut hole = self.snap(rect).inset(radius.max(0.) * 0.3 + 2.);
        if let Some([_, top, bottom, _]) = self.fade.filter(|fade| fade[1] > fade[0]) {
            let y0 = hole.y.max(top);
            let y1 = (hole.y + hole.h).min(bottom);
            hole = Rect::new(hole.x, y0, hole.w, y1 - y0);
        }
        (hole.w > 0. && hole.h > 0.).then_some(hole)
    }

    /// A texture holding a shadow's coverage at each drawable pixel of its snapped quad,
    /// computed with the paint shader's formula: drawn 1:1, it reproduces that shader's
    /// output up to 8-bit rounding, without its per-fragment distance and smoothstep.
    /// None for very large shadows, which keep the shader path.
    fn shadow_texture(&mut self, quad: Rect, radius: f32, blur: f32) -> Option<GLuint> {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*ENABLED.get_or_init(|| std::env::var("LUMA_SHADOW_TEXTURES").as_deref() != Ok("0"))
            || blur <= 0.
        {
            return None;
        }
        let key = self.shadow_key_of(quad, radius, blur);
        let quad = self.snap(quad);
        let scale = self.stats.viewport.scale.max(0.01);
        let (width, height) = (key.0, key.1);
        let texels = width as usize * height as usize;
        if width == 0 || height == 0 || texels > SHADOW_TEXELS {
            return None;
        }
        let frame = self.frame;
        if let Some(texture) = self.shadows.get_mut(&key) {
            texture.frame = frame;
            return Some(texture.id);
        }
        // Computing coverage costs CPU time a frame cannot spare: draw this one with the
        // shader and prepare the texture while idle (see prepare_idle).
        if !self.preparing_shadow {
            if !self
                .pending_shadows
                .iter()
                .any(|(q, r, b)| q.w == quad.w && q.h == quad.h && *r == radius && *b == blur)
            {
                self.pending_shadows.push((quad, radius, blur));
            }
            return None;
        }
        let mut used: usize = self
            .shadows
            .values()
            .map(|t| t.width as usize * t.height as usize)
            .sum();
        if used + texels > SHADOW_BUDGET {
            let mut old: Vec<_> = self
                .shadows
                .iter()
                .filter(|(_, t)| t.frame != frame)
                .map(|(key, t)| (*key, t.frame))
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (key, _) in old {
                if used + texels <= SHADOW_BUDGET {
                    break;
                }
                if let Some(t) = self.shadows.remove(&key) {
                    used -= t.width as usize * t.height as usize;
                }
            }
            if used + texels > SHADOW_BUDGET {
                return None;
            }
        }
        let started = Instant::now();
        let pixels = shadow_mask(quad, radius, blur, scale, width, height);
        self.stats.image_load_ms += started.elapsed().as_secs_f64() * 1000.;
        let mut texture = if self.info.version.starts_with("OpenGL ES 3") {
            upload_texture(width, height, &pixels, true)
        } else {
            let rgba: Vec<u8> = pixels
                .iter()
                .flat_map(|&alpha| [255, 255, 255, alpha])
                .collect();
            texture(width, height, &rgba)
        };
        texture.frame = frame;
        let id = texture.id;
        self.shadows.insert(key, texture);
        Some(id)
    }

    /// Processes one pending shadow. Its cached coverage may now differ from the shader.
    /// Call when nothing animates: the next frame draws that shadow from the texture.
    /// Returns the texture's key: only shadows with that key look different now.
    pub fn prepare_idle(&mut self) -> Option<ShadowKey> {
        let (quad, radius, blur) = self.pending_shadows.pop()?;
        self.preparing_shadow = true;
        self.shadow_texture(quad, radius, blur);
        self.preparing_shadow = false;
        Some(self.shadow_key_of(quad, radius, blur))
    }

    /// The cached coverage texture a shadow command draws from, once prepared.
    pub fn shadow_key(&self, draw: &Draw) -> Option<ShadowKey> {
        match draw {
            Draw::Shadow(rect, radius, blur, _) if *blur > 0. => {
                Some(self.shadow_key_of(rect.inset(-*blur), *radius, *blur))
            }
            _ => None,
        }
    }

    fn shadow_key_of(&self, quad: Rect, radius: f32, blur: f32) -> ShadowKey {
        let quad = self.snap(quad);
        let scale = self.stats.viewport.scale.max(0.01);
        (
            (quad.w * scale).round() as u32,
            (quad.h * scale).round() as u32,
            radius.to_bits(),
            blur.to_bits(),
        )
    }

    /// Whether an image path already has a texture (or a cached failure).
    pub fn has_image(&self, path: &str) -> bool {
        self.textures.contains_key(path)
    }

    /// Loads a vector logo's mesh ahead of its first draw. True when it was not loaded yet.
    pub fn preload_vector(&mut self, path: &str) -> bool {
        if self.vectors.contains_key(path) {
            return false;
        }
        let mesh = vector::Mesh::load(Path::new(path))
            .map_err(|error| eprintln!("{error}"))
            .ok();
        self.vectors.insert(path.into(), mesh);
        true
    }

    /// Uploads an image decoded elsewhere, so its first draw does not decode it.
    pub fn adopt_image(&mut self, path: String, image: Option<image::RgbaImage>) {
        if self.textures.contains_key(&path) {
            return;
        }
        let texture = image.map(|image| {
            let mut texture = texture(image.width(), image.height(), image.as_raw());
            texture.frame = self.frame;
            texture
        });
        self.textures.insert(path, texture);
    }

    /// Appends `count` paint vertices from `start`, merged into the previous batch when it uses
    /// the same texture, clip and fade.
    fn paint_batch(
        &mut self,
        start: usize,
        count: usize,
        texture: GLuint,
        backdrop: Option<[[u8; 3]; 2]>,
        opaque: bool,
    ) {
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
                    count: batch_count,
                    backdrop: previous_backdrop,
                    opaque: previous_opaque,
                    ..
                } = &mut batch.kind
                && *previous == texture
                && *previous_backdrop == backdrop
                && *previous_opaque == opaque
            {
                *batch_count += count;
                return;
            }
        }
        self.batches.push(Batch {
            kind: BatchKind::Paint {
                texture,
                start,
                count,
                backdrop,
                opaque,
            },
            clip: self.clip,
            fade: self.fade,
        });
    }

    /// The wallpaper with full-screen translucent fills folded in: each fill maps a pixel p to
    /// p * (1 - a) + c * a, so together they are p * m + k, drawn in one pass instead of one
    /// full-screen layer each. Strips keep gradients' products of colour and alpha exact.
    fn folded_quad(&mut self, rect: Rect, texture: GLuint, scrims: &[Scrim], opaque: bool) {
        const STRIPS: usize = 16;
        let fold = |t: f32| {
            let (mut m, mut k) = ([1f32; 3], [0f32; 3]);
            for scrim in scrims {
                let color = scrim.at(t);
                for i in 0..3 {
                    m[i] *= 1. - color[3];
                    k[i] = k[i] * (1. - color[3]) + color[i] * color[3];
                }
            }
            (m, k)
        };
        let start = self.vertices.len() / 14;
        for strip in 0..STRIPS {
            let rows = [
                strip as f32 / STRIPS as f32,
                (strip + 1) as f32 / STRIPS as f32,
            ];
            let folds = rows.map(fold);
            for (x, row) in [(0., 0), (1., 0), (0., 1), (0., 1), (1., 0), (1., 1)] {
                let (m, k) = folds[row];
                let t = rows[row];
                self.vertices.extend_from_slice(&[
                    rect.x + x * rect.w,
                    rect.y + t * rect.h,
                    x,
                    t,
                    m[0],
                    m[1],
                    m[2],
                    1.,
                    k[0],
                    k[1],
                    k[2],
                    0.,
                    -2.,
                    0.,
                ]);
            }
        }
        self.paint_batch(
            start,
            STRIPS * 6,
            texture,
            None,
            opaque && self.fade.is_none(),
        );
    }

    fn text(&mut self, text: &str, rect: Rect, style: TextStyle, color: Color, centered: bool) {
        let viewport = self.stats.viewport;
        // Keep layout logical, but use the same pixel-sized em for metrics and
        // outlines. The effective size in TextKey separates density-dependent runs.
        let size_f = physical_font_size(
            style.size,
            viewport.text_scale(),
            self.stats.font_size_snapping,
        );
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
                | (u8::from(self.stats.multisample_samples > 1) << 2)
                | (u8::from(style.size >= 32.) << 3),
        };
        if self.text_runs.contains_key(&key) {
            self.submit_text(key, origin, color);
            return;
        }
        let rect = Rect::new(0., 0., rect.w, rect.h);
        // Preserve the view's optical-font choice even if quantization crosses
        // the 32px threshold. The choice is also part of the run cache key.
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

    fn image(&mut self, path: &str, rect: Rect, radius: f32, contain: bool, tint: Color) {
        self.folded_image(path, rect, radius, contain, tint, &[]);
    }

    fn folded_image(
        &mut self,
        path: &str,
        mut rect: Rect,
        radius: f32,
        contain: bool,
        tint: Color,
        scrims: &[Scrim],
    ) {
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
        let started = Instant::now();
        let loaded = !self.textures.contains_key(&path);
        let entry = self.textures.entry(path.clone()).or_insert_with(|| {
            let decoded = match crate::preload::take(&path) {
                Some(Some(image)) => Ok(image),
                Some(None) | None => image::open(&path).map(image::DynamicImage::into_rgba8),
            };
            match decoded {
                Ok(image) => Some(texture(image.width(), image.height(), image.as_raw())),
                Err(error) => {
                    eprintln!("Artwork {path}: {error}");
                    None
                }
            }
        });
        if loaded {
            self.stats.image_load_ms += started.elapsed().as_secs_f64() * 1000.;
        }
        let Some(texture) = entry else {
            self.stats.missing_images += 1;
            return;
        };
        texture.frame = frame;
        let id = texture.id;
        let opaque = texture.opaque && tint[3] >= 1.;
        if contain {
            let scale = (rect.w / texture.width as f32).min(rect.h / texture.height as f32);
            let w = texture.width as f32 * scale;
            let h = texture.height as f32 * scale;
            rect = Rect::new(rect.x + (rect.w - w) / 2., rect.y + (rect.h - h) / 2., w, h);
        }
        if scrims.is_empty() {
            let hole = opaque
                .then(|| self.opaque_interior(rect, radius))
                .flatten()
                .map(Hole::Opaque);
            self.gradient_quad(
                rect,
                [0., 0., 1., 1.],
                plain_radius(radius),
                0.,
                [tint; 2],
                id,
                None,
                hole,
            );
        } else {
            self.folded_quad(rect, id, scrims, opaque);
        }
    }

    /// Edges on whole drawable pixels. The TV scales its 1080p graphics plane up 2x, which
    /// turns a half-covered edge pixel into a soft 2-pixel band; whole pixels stay crisp.
    /// LUMA_SNAP=0 keeps logical positions (for comparison).
    fn snap(&self, rect: Rect) -> Rect {
        snap_rect(rect, self.stats.viewport.scale)
    }

    fn vector_image(&mut self, path: &str, mut rect: Rect, contain: bool, tint: Color) {
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        let scale = self.stats.viewport.scale;
        let started = Instant::now();
        let loaded = !self.vectors.contains_key(path);
        let mesh = self.vectors.entry(path.into()).or_insert_with(|| {
            match vector::Mesh::load(Path::new(path)) {
                Ok(mesh) => Some(mesh),
                Err(error) => {
                    eprintln!("{error}");
                    None
                }
            }
        });
        if loaded {
            self.stats.vector_load_ms += started.elapsed().as_secs_f64() * 1000.;
        }
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
        // Logos keep their size; only their placement moves onto the pixel grid.
        let snapped = snap_rect(rect, scale);
        rect = Rect::new(snapped.x, snapped.y, rect.w, rect.h);
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

    fn app_image(
        &mut self,
        image: &crate::app_artwork::Artwork,
        rect: Rect,
        radius: f32,
        tint: Color,
    ) {
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
        let scale =
            (rect.w * 0.68 / texture.width as f32).min(rect.h * 0.68 / texture.height as f32);
        let w = texture.width as f32 * scale;
        let h = texture.height as f32 * scale;
        let id = texture.id;
        self.gradient_quad(
            rect,
            [
                0.5 - rect.w / (2. * w),
                0.5 - rect.h / (2. * h),
                0.5 + rect.w / (2. * w),
                0.5 + rect.h / (2. * h),
            ],
            radius,
            0.,
            [tint; 2],
            id,
            Some(image.backdrop),
            (tint[3] >= 1.)
                .then(|| self.opaque_interior(rect, radius))
                .flatten()
                .map(Hole::Opaque),
        );
    }

    /// A logical damage rect in drawable pixels ([x, y, width, height], origin bottom-left),
    /// grown to whole pixels.
    pub fn damage_pixels(rect: Rect, width: u32, height: u32) -> [i32; 4] {
        let viewport = Viewport::new(Size::new(width, height));
        let sx = viewport.width as f32 / 1920.;
        let sy = viewport.height as f32 / 1080.;
        let x0 = (viewport.x as f32 + rect.x.max(0.) * sx).floor() as i32;
        let x1 = (viewport.x as f32 + (rect.x + rect.w).min(1920.) * sx).ceil() as i32;
        let y0 = (viewport.y as f32 + (1080. - (rect.y + rect.h).min(1080.)) * sy).floor() as i32;
        let y1 = (viewport.y as f32 + (1080. - rect.y.max(0.)) * sy).ceil() as i32;
        if x1 <= x0 || y1 <= y0 {
            // No change still names, clears and redraws one pixel inside the drawable. An empty
            // or off-surface region left the driver nothing to preserve: Mali presented black.
            return [viewport.x, viewport.y, 1, 1];
        }
        [x0, y0, x1 - x0, y1 - y0]
    }

    /// The logical area drawable pixels from `damage_pixels` cover. Grown to whole pixels it
    /// can exceed the original logical damage; commands are culled against this area, so none
    /// that paints into the cleared pixels is skipped.
    pub fn damage_area(pixels: [i32; 4], width: u32, height: u32) -> Rect {
        let viewport = Viewport::new(Size::new(width, height));
        let sx = viewport.width as f32 / 1920.;
        let sy = viewport.height as f32 / 1080.;
        let [x, y, w, h] = pixels;
        Rect::new(
            (x - viewport.x) as f32 / sx,
            1080. - (y + h - viewport.y) as f32 / sy,
            w as f32 / sx,
            h as f32 / sy,
        )
    }

    /// Prepares first-use work for a scene without drawing it (text layout and glyph
    /// outlines, logo meshes, images), e.g. another page while the screen is idle, so its
    /// first appearance does not pay for it. The visible frame's statistics stay unchanged.
    pub fn prepare(&mut self, commands: &[Draw], width: u32, height: u32) {
        let stats = self.stats.clone();
        self.build(commands, width, height, None);
        self.vertices.clear();
        self.batches.clear();
        self.stats = stats;
    }

    /// Cache a bounded, prefiltered backdrop once per wallpaper, not once per surface/frame.
    /// Wallpaper-only sampling avoids framebuffer feedback and works with partial redraw on GLES2.
    fn glass_backdrop(&mut self, path: &str) -> GLuint {
        let started = Instant::now();
        let key = format!("glass:{path}");
        let loaded = !self.textures.contains_key(&key);
        let mut precomputed = false;
        let mut generated = false;
        let entry = self.textures.entry(key).or_insert_with(|| {
            // Shipped assets and new uploads prepare this off the SDL thread. Keep the
            // exact legacy path for older uploads, missing/corrupt sidecars and A/B checks.
            let prepared = (std::env::var("LUMA_GLASS_PRECOMPUTED").as_deref() != Ok("0"))
                .then(|| {
                    let sidecar = crate::glass_backdrop::sidecar_path(Path::new(path));
                    match crate::preload::take(&sidecar.to_string_lossy()) {
                        Some(image) => image,
                        None => crate::glass_backdrop::load(&sidecar).ok(),
                    }
                })
                .flatten();
            precomputed = prepared.is_some();
            let atlas = prepared.or_else(|| {
                let image = image::open(path).ok()?;
                generated = true;
                Some(crate::glass_backdrop::prepare(image))
            })?;
            Some(texture(atlas.width(), atlas.height(), atlas.as_raw()))
        });
        if loaded {
            self.stats.glass_prepare_ms += started.elapsed().as_secs_f64() * 1000.;
            self.stats.glass_precomputed_loads += usize::from(precomputed);
            self.stats.glass_runtime_prepares += usize::from(generated);
        }
        if let Some(texture) = entry {
            texture.frame = self.frame;
            texture.id
        } else {
            self.white.id
        }
    }

    /// Lays out the scene into vertices and batches; commands entirely outside `area` are
    /// skipped.
    fn build(&mut self, commands: &[Draw], width: u32, height: u32, area: Option<Rect>) {
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
        let mut index = 0;
        while index < commands.len() {
            let command = &commands[index];
            index += 1;
            if profile_skip(command) {
                continue;
            }
            if let Some(path) = plain_wallpaper(command)
                && self.clip.is_none()
                && self.fade.is_none()
            {
                let mut scrims = Vec::new();
                while let Some(next) = commands.get(index) {
                    if profile_skip(next) {
                        index += 1;
                    } else if let Some(scrim) = Scrim::of(next) {
                        scrims.push(scrim);
                        index += 1;
                    } else {
                        break;
                    }
                }
                if !scrims.is_empty() {
                    let screen = Rect::new(0., 0., 1920., 1080.);
                    self.folded_image(path, screen, 0., false, [1.; 4], &scrims);
                    continue;
                }
            }
            // Outside the damage the scissor would discard it anyway; skip its layout too.
            if let (Some(area), Some(bounds)) = (area, crate::damage::bounds(command))
                && !crate::damage::intersects(area, bounds)
            {
                continue;
            }
            // Likewise outside the clip, e.g. list rows scrolled out of view: the scissor
            // discards their pixels, but submitting them still costs CPU time per draw.
            if let (Some(clip), Some(bounds)) = (self.clip, crate::damage::bounds(command))
                && !crate::damage::intersects(clip.inset(-2.), bounds)
            {
                continue;
            }
            match command {
                Draw::Glass(rect, radius, tint) => self.quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    -1000.,
                    *tint,
                    self.white.id,
                ),
                // A border's coverage is exactly zero more than its width (plus the
                // filter) inside the edge.
                Draw::Shape(rect, radius, border, color) => self.gradient_quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    *border,
                    [*color; 2],
                    self.white.id,
                    None,
                    (*border > 0.)
                        .then(|| Hole::Skip(self.snap(*rect).inset(radius + border + 2.))),
                ),
                Draw::RoundedGradient(rect, radius, top, bottom) => self.gradient_quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    0.,
                    [*top, *bottom],
                    self.white.id,
                    None,
                    None,
                ),
                Draw::Shadow(rect, radius, blur, color) => {
                    let hole = self.cover_after(&commands[index..], *rect).map(Hole::Skip);
                    match self.shadow_texture(rect.inset(-*blur), *radius, *blur) {
                        // The same coverage, read instead of computed per fragment.
                        Some(texture) => self.gradient_quad(
                            rect.inset(-*blur),
                            [0., 0., 1., 1.],
                            -1.,
                            0.,
                            [*color; 2],
                            texture,
                            None,
                            hole,
                        ),
                        None => self.gradient_quad(
                            rect.inset(-*blur),
                            [0., 0., 1., 1.],
                            *radius,
                            -*blur,
                            [*color; 2],
                            self.white.id,
                            None,
                            hole,
                        ),
                    }
                }
                Draw::Text(text, rect, size, color, centered) => {
                    self.text(text, *rect, *size, *color, *centered)
                }
                Draw::Image(path, rect, radius, contain) => {
                    self.image(path, *rect, *radius, *contain, [1.; 4])
                }
                Draw::TintedImage(path, rect, radius, contain, tint) => {
                    self.image(path, *rect, *radius, *contain, *tint)
                }
                Draw::AppImage(image, rect, radius, tint) => {
                    self.app_image(image, *rect, *radius, *tint)
                }
                Draw::Clip(clip) => self.clip = *clip,
                Draw::EdgeFade(fade) => self.fade = *fade,
            }
        }
    }

    /// Draws the scene. With `damage` (drawable pixels from `damage_pixels`), only those pixels
    /// are cleared and drawn; commands entirely outside them are skipped, and the rest of the
    /// back buffer keeps its earlier contents.
    pub fn render(&mut self, commands: &[Draw], width: u32, height: u32, damage: Option<[i32; 4]>) {
        let area = damage.map(|pixels| Self::damage_area(pixels, width, height));
        let started = Instant::now();
        self.build(commands, width, height, area);
        let mut backdrops = commands.iter().filter_map(|draw| match draw {
            Draw::Image(path, rect, _, false) if full_screen(rect) => Some((path.as_str(), 1.)),
            Draw::TintedImage(path, rect, _, false, tint) if full_screen(rect) => {
                Some((path.as_str(), tint[3]))
            }
            _ => None,
        });
        let first = backdrops.next();
        let second = backdrops.next();
        let has_glass = commands.iter().any(|draw| matches!(draw, Draw::Glass(..)));
        let glass_enabled = has_glass
            && self.info.fragment_highp
            && std::env::var("LUMA_GLASS").as_deref() != Ok("0");
        self.stats.glass_surfaces = commands
            .iter()
            .filter(|draw| matches!(draw, Draw::Glass(..)))
            .count();
        self.stats.glass_backdrop_sampling = glass_enabled;
        let glass_a = if glass_enabled {
            first
                .map(|(path, _)| self.glass_backdrop(path))
                .unwrap_or(self.white.id)
        } else {
            self.white.id
        };
        let glass_b = if glass_enabled {
            second
                .map(|(path, _)| self.glass_backdrop(path))
                .unwrap_or(glass_a)
        } else {
            glass_a
        };
        let Viewport {
            x: vx,
            y: vy,
            width: vw,
            height: vh,
            scale,
        } = self.stats.viewport;
        if let Some([_, _, w, h]) = damage {
            self.stats.redrawn_fraction = f64::from(w * h) / f64::from((width * height).max(1));
        }
        // Only changes reach the driver: each state call costs real time on the TV.
        let mut scissor_state: Option<Option<[i32; 4]>> = None;
        let mut scissor = |area: Option<[i32; 4]>| unsafe {
            if scissor_state == Some(area) {
                return;
            }
            let enabled = scissor_state.flatten().is_some();
            scissor_state = Some(area);
            match area {
                Some([x, y, w, h]) => {
                    if !enabled {
                        gl::Enable(gl::SCISSOR_TEST);
                    }
                    gl::Scissor(x, y, w.max(0), h.max(0));
                }
                None => gl::Disable(gl::SCISSOR_TEST),
            }
        };
        unsafe {
            gl::Viewport(0, 0, width as i32, height as i32);
            scissor(damage);
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
            gl::Uniform1i(
                uniform_location(self.program, c"u_glass_enabled".as_ptr()),
                i32::from(glass_enabled),
            );
            gl::Uniform1i(uniform_location(self.program, c"u_glass_a".as_ptr()), 1);
            gl::Uniform1i(uniform_location(self.program, c"u_glass_b".as_ptr()), 2);
            gl::Uniform1f(
                uniform_location(self.program, c"u_glass_mix".as_ptr()),
                second.map_or(0., |(_, alpha)| alpha),
            );
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, glass_a);
            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, glass_b);
            // One drawable pixel in logical units: the paint shader's edge filter width.
            gl::Uniform1f(
                uniform_location(self.program, c"u_aa".as_ptr()),
                1. / scale.max(0.01),
            );
            gl::Uniform1i(uniform_location(self.program, c"u_texture".as_ptr()), 0);
            gl::ActiveTexture(gl::TEXTURE0);
            let mut paint_bound = false;
            let mut active_paint = 0;
            let mut shape_configured = false;
            let mut mesh_state = MeshState::default();
            let mut current_fade = None;
            let mut blending = true;
            let mut bound_texture = 0;
            // u_app of the paint program (it keeps its value across program switches).
            let mut app_uniform: Option<bool> = None;
            for batch in &self.batches {
                if current_fade != Some(batch.fade) {
                    // The same cheap alpha mask applies to shapes, textures,
                    // icons and text; the unmasked wallpaper remains beneath.
                    let fade = batch.fade.unwrap_or([0., 0., 0., 0.]);
                    for program in [self.program, self.vector_program, self.text_program] {
                        gl::UseProgram(program);
                        gl::Uniform4f(
                            uniform_location(program, c"u_edge_fade".as_ptr()),
                            fade[0],
                            fade[1],
                            fade[2],
                            fade[3],
                        );
                    }
                    current_fade = Some(batch.fade);
                    paint_bound = false;
                    mesh_state.program = 0;
                }
                let clip = batch.clip.map(|clip| {
                    [
                        vx + (clip.x * vw as f32 / 1920.).round() as i32,
                        vy + ((1080. - clip.y - clip.h) * vh as f32 / 1080.).round() as i32,
                        (clip.w * vw as f32 / 1920.).round() as i32,
                        (clip.h * vh as f32 / 1080.).round() as i32,
                    ]
                });
                scissor(match (clip, damage) {
                    (Some(a), Some(b)) => Some(intersect(a, b)),
                    (a, b) => a.or(b),
                });
                match &batch.kind {
                    BatchKind::Paint {
                        texture,
                        start,
                        count,
                        backdrop,
                        opaque,
                    } => {
                        if *opaque == blending {
                            if *opaque {
                                gl::Disable(gl::BLEND);
                            } else {
                                gl::Enable(gl::BLEND);
                            }
                            blending = !*opaque;
                        }
                        let program = self
                            .paint_programs
                            .as_ref()
                            .filter(|_| batch.fade.is_none())
                            .map_or(self.program, |programs| {
                                programs.select(
                                    &self.vertices[start * 14..(start + count) * 14],
                                    *opaque,
                                    backdrop.is_some(),
                                    self.program,
                                )
                            });
                        if !paint_bound || active_paint != program {
                            gl::UseProgram(program);
                            active_paint = program;
                        }
                        if !shape_configured
                            && self
                                .paint_programs
                                .as_ref()
                                .is_some_and(|p| p.shape == program)
                        {
                            gl::Uniform1f(
                                uniform_location(program, c"u_aa".as_ptr()),
                                1. / scale.max(0.01),
                            );
                            shape_configured = true;
                        }
                        if !paint_bound && self.paint_array != 0 {
                            // The paint attributes, recorded once: one bind after a mesh.
                            gl::BindVertexArray(self.paint_array);
                            mesh_state.vertex_array = None;
                            mesh_state.program = 0;
                            paint_bound = true;
                        }
                        if !paint_bound {
                            mesh_state.program = 0;
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
                        if bound_texture != *texture {
                            gl::BindTexture(gl::TEXTURE_2D, *texture);
                            bound_texture = *texture;
                        }
                        if program == self.program && app_uniform != Some(backdrop.is_some()) {
                            gl::Uniform1i(
                                uniform_location(self.program, c"u_app".as_ptr()),
                                i32::from(backdrop.is_some()),
                            );
                            app_uniform = Some(backdrop.is_some());
                        }
                        if let Some(stops) = backdrop {
                            for (name, rgb) in
                                [(c"u_app_top", stops[0]), (c"u_app_bottom", stops[1])]
                            {
                                gl::Uniform3f(
                                    uniform_location(program, name.as_ptr()),
                                    rgb[0] as f32 / 255.,
                                    rgb[1] as f32 / 255.,
                                    rgb[2] as f32 / 255.,
                                );
                            }
                        }
                        gl::DrawArrays(gl::TRIANGLES, *start as i32, *count as i32);
                    }
                    BatchKind::Vector { .. } | BatchKind::Text { .. } if !blending => {
                        gl::Enable(gl::BLEND);
                        blending = true;
                    }
                    _ => {}
                }
                match &batch.kind {
                    BatchKind::Paint { .. } => {}
                    BatchKind::Vector { path, rect, tint } => {
                        let Some(mesh) = self.vectors.get(path).and_then(Option::as_ref) else {
                            continue;
                        };
                        self.draw_mesh(
                            &mut mesh_state,
                            self.vector_program,
                            mesh,
                            [rect.w / mesh.width, rect.h / mesh.height, rect.x, rect.y],
                            *tint,
                            0.,
                        );
                        paint_bound = false;
                    }
                    BatchKind::Text { key, origin, tint } => {
                        let Some(mesh) = self.text_runs.get(key).and_then(|run| run.mesh.as_ref())
                        else {
                            continue;
                        };
                        let size = f32::from_bits(key.layout[0]) * vh as f32 / 1080.;
                        self.draw_mesh(
                            &mut mesh_state,
                            self.text_program,
                            mesh,
                            [1., 1., origin[0], origin[1]],
                            *tint,
                            embolden(size, self.text_embolden),
                        );
                        paint_bound = false;
                    }
                }
            }
            if !blending {
                gl::Enable(gl::BLEND);
            }
            if self.vertex_arrays {
                gl::BindVertexArray(self.vao);
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

    fn draw_mesh(
        &self,
        state: &mut MeshState,
        program: GLuint,
        mesh: &vector::Mesh,
        transform: [f32; 4],
        tint: Color,
        embolden: f32,
    ) {
        let viewport = self.stats.viewport;
        let scale = [
            viewport.width as f32 / 1920.,
            viewport.height as f32 / 1080.,
        ];
        unsafe {
            if state.program != program {
                gl::UseProgram(program);
                state.program = program;
            }
            if program == self.text_program {
                let fringe = if self.stats.multisample_samples > 1 {
                    0.
                } else {
                    0.5
                };
                if state.fringe != Some(fringe) {
                    gl::Uniform1f(uniform_location(program, c"u_fringe".as_ptr()), fringe);
                    state.fringe = Some(fringe);
                }
                if state.embolden != Some(embolden) {
                    gl::Uniform1f(uniform_location(program, c"u_embolden".as_ptr()), embolden);
                    state.embolden = Some(embolden);
                }
            }
            if mesh.vertex_array != 0 {
                if state.vertex_array != Some(mesh.vertex_array) {
                    gl::BindVertexArray(mesh.vertex_array);
                    state.vertex_array = Some(mesh.vertex_array);
                }
            } else {
                if self.vertex_arrays && state.vertex_array.is_some() {
                    gl::BindVertexArray(self.vao);
                    state.vertex_array = None;
                }
                vector::bind_attributes(mesh.buffer);
            }
            gl::Uniform4f(
                uniform_location(program, c"u_transform".as_ptr()),
                transform[0],
                transform[1],
                transform[2],
                transform[3],
            );
            let known = if program == self.text_program {
                &mut state.text_scale
            } else {
                &mut state.vector_scale
            };
            if *known != Some(scale) {
                gl::Uniform2f(
                    uniform_location(program, c"u_pixel_scale".as_ptr()),
                    scale[0],
                    scale[1],
                );
                *known = Some(scale);
            }
            gl::Uniform4f(
                uniform_location(program, c"u_tint".as_ptr()),
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
        self.render(commands, size.width, size.height, None);
        let stats = self.stats.clone();
        let result = self.screenshot(path, size.width, size.height);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteFramebuffers(1, &framebuffer);
        }
        result.map(|_| stats)
    }

    /// Verification aid (LUMA_DAMAGE_VERIFY=1): compares the window's back buffer after a
    /// partial redraw with the same scene drawn in full offscreen at the window's sample count.
    /// Returns the pixels differing by more than 16 in a channel, and the largest difference.
    pub fn verify_window(
        &mut self,
        commands: &[Draw],
        width: u32,
        height: u32,
        save: Option<&Path>,
    ) -> (usize, u8) {
        let (w, h) = (width as i32, height as i32);
        let length = width as usize * height as usize * 4;
        let mut window = vec![0u8; length];
        let mut full = vec![0u8; length];
        let samples = self.stats.multisample_samples as i32;
        let stats = self.stats.clone();
        let mut framebuffers = [0; 2];
        let mut renderbuffers = [0; 2];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::ReadPixels(
                0,
                0,
                w,
                h,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                window.as_mut_ptr().cast(),
            );
            gl::GenFramebuffers(2, framebuffers.as_mut_ptr());
            gl::GenRenderbuffers(2, renderbuffers.as_mut_ptr());
            gl::BindRenderbuffer(gl::RENDERBUFFER, renderbuffers[0]);
            if samples > 1 {
                gl::RenderbufferStorageMultisample(gl::RENDERBUFFER, samples, gl::RGBA8, w, h);
            } else {
                gl::RenderbufferStorage(gl::RENDERBUFFER, gl::RGBA8, w, h);
            }
            gl::BindRenderbuffer(gl::RENDERBUFFER, renderbuffers[1]);
            gl::RenderbufferStorage(gl::RENDERBUFFER, gl::RGBA8, w, h);
            for (framebuffer, renderbuffer) in framebuffers.into_iter().zip(renderbuffers) {
                gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer);
                gl::FramebufferRenderbuffer(
                    gl::FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0,
                    gl::RENDERBUFFER,
                    renderbuffer,
                );
            }
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffers[0]);
        }
        self.render(commands, width, height, None);
        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, framebuffers[0]);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, framebuffers[1]);
            gl::BlitFramebuffer(0, 0, w, h, 0, 0, w, h, gl::COLOR_BUFFER_BIT, gl::NEAREST);
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffers[1]);
            gl::ReadPixels(
                0,
                0,
                w,
                h,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                full.as_mut_ptr().cast(),
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteFramebuffers(2, framebuffers.as_ptr());
            gl::DeleteRenderbuffers(2, renderbuffers.as_ptr());
        }
        self.stats = stats;
        let mut differing = 0;
        let mut largest = 0;
        for (a, b) in window.chunks_exact(4).zip(full.chunks_exact(4)) {
            let difference = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0);
            largest = largest.max(difference);
            differing += usize::from(difference > 16);
        }
        if let Some(stem) = save.filter(|_| differing > 0) {
            for (name, pixels) in [("window", window), ("full", full)] {
                if let Some(image) = image::RgbaImage::from_raw(width, height, pixels) {
                    let image = image::imageops::flip_vertical(&image);
                    let _ = image.save(stem.with_extension(format!("{name}.png")));
                }
            }
        }
        (differing, largest)
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
            if self.paint_array != 0 {
                gl::DeleteVertexArrays(1, &self.paint_array);
            }
        }
    }
}

fn shadow_mask(quad: Rect, radius: f32, blur: f32, scale: f32, width: u32, height: u32) -> Vec<u8> {
    // As paint.frag: v_shape.zw is the quad's half size, the box is that minus blur.
    let (hx, hy) = (quad.w / 2. - blur, quad.h / 2. - blur);
    let r = radius.min(hx.min(hy));
    let coverage = |qx: f32, qy: f32| {
        let (ox, oy) = (qx.max(0.), qy.max(0.));
        let d = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.) - r;
        let t = ((d + blur) / (2. * blur)).clamp(0., 1.);
        ((1. - t * t * (3. - 2. * t)) * 255.).round() as u8
    };
    // The coverage is symmetric about both centre lines: compute one quarter (plus the
    // centre row and column of odd sizes) and mirror it.
    let (w, h) = (width as usize, height as usize);
    let mut pixels = vec![255u8; w * h];
    for row in 0..h.div_ceil(2) {
        let y = -quad.h / 2. + (row as f32 + 0.5) / scale;
        let qy = y.abs() - hy + r;
        for column in 0..w.div_ceil(2) {
            let x = -quad.w / 2. + (column as f32 + 0.5) / scale;
            let qx = x.abs() - hx + r;
            let value = coverage(qx, qy);
            // Past this column the distance depends only on y, or the
            // rounded coverage has already saturated at full opacity.
            if value == 255 || qx <= qy.min(0.) {
                if value != 255 {
                    pixels[row * w + column..row * w + w - column].fill(value);
                }
                break;
            }
            pixels[row * w + column] = value;
            pixels[row * w + w - 1 - column] = value;
        }
        let mirror = h - 1 - row;
        if mirror != row {
            pixels.copy_within(row * w..(row + 1) * w, mirror * w);
        }
    }
    pixels
}

fn texture(width: u32, height: u32, pixels: &[u8]) -> Texture {
    upload_texture(width, height, pixels, false)
}

fn upload_texture(width: u32, height: u32, pixels: &[u8], coverage: bool) -> Texture {
    let mut id = 0;
    unsafe {
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        if coverage {
            // Preserve the original white RGB and filtered coverage in alpha.
            for channel in [
                gl::TEXTURE_SWIZZLE_R,
                gl::TEXTURE_SWIZZLE_G,
                gl::TEXTURE_SWIZZLE_B,
            ] {
                gl::TexParameteri(gl::TEXTURE_2D, channel, gl::ONE as i32);
            }
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_SWIZZLE_A, gl::RED as i32);
        }
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            if coverage { gl::R8 } else { gl::RGBA } as i32,
            width as i32,
            height as i32,
            0,
            if coverage { gl::RED } else { gl::RGBA },
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
        opaque: !coverage
            && !pixels.is_empty()
            && pixels.chunks_exact(4).all(|texel| texel[3] == 255),
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

/// Where linked program binaries are kept between launches (webOS only): compiling the
/// shaders is a large part of Luma's startup on TV GPUs.
static PROGRAM_CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_program_cache(directory: PathBuf) {
    let _ = PROGRAM_CACHE.set(directory);
}

/// The cache file for these sources on this exact driver; any change misses the cache.
fn program_cache_file(vertex: &str, fragment: &str, attributes: &[&CStr]) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let directory = PROGRAM_CACHE.get()?;
    if !gl::ProgramBinary::is_loaded() || !gl::GetProgramBinary::is_loaded() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for name in [gl::VENDOR, gl::RENDERER, gl::VERSION] {
        gl_string(name).hash(&mut hasher);
    }
    (vertex, fragment, attributes).hash(&mut hasher);
    Some(directory.join(format!("{:016x}.bin", hasher.finish())))
}

fn load_program_binary(file: &Path) -> Option<GLuint> {
    let bytes = std::fs::read(file).ok()?;
    let format = GLenum::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
    let binary = &bytes[4..];
    unsafe {
        let program = gl::CreateProgram();
        gl::ProgramBinary(program, format, binary.as_ptr().cast(), binary.len() as i32);
        let mut success = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
        if success == 0 {
            // A driver update or a damaged file: compile from source instead.
            gl::DeleteProgram(program);
            let _ = std::fs::remove_file(file);
            return None;
        }
        Some(program)
    }
}

fn save_program_binary(program: GLuint, file: &Path) {
    unsafe {
        let mut length = 0;
        gl::GetProgramiv(program, gl::PROGRAM_BINARY_LENGTH, &mut length);
        if length <= 0 {
            return;
        }
        let mut binary = vec![0u8; length as usize];
        let mut written = 0;
        let mut format = 0;
        gl::GetProgramBinary(
            program,
            length,
            &mut written,
            &mut format,
            binary.as_mut_ptr().cast(),
        );
        if written <= 0 {
            return;
        }
        binary.truncate(written as usize);
        let mut bytes = format.to_le_bytes().to_vec();
        bytes.extend_from_slice(&binary);
        // Write beside, then rename: a crash never leaves a truncated binary in place.
        let partial = file.with_extension("partial");
        if file
            .parent()
            .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
            && std::fs::write(&partial, bytes).is_ok()
        {
            let _ = std::fs::rename(&partial, file);
        }
    }
}

fn link(vertex: &str, fragment: &str, attributes: &[&CStr]) -> Result<GLuint, String> {
    let cache = program_cache_file(vertex, fragment, attributes);
    if let Some(program) = cache.as_deref().and_then(load_program_binary) {
        return Ok(program);
    }
    let program = link_source(vertex, fragment, attributes)?;
    if let Some(file) = cache {
        save_program_binary(program, &file);
    }
    Ok(program)
}

fn link_source(vertex: &str, fragment: &str, attributes: &[&CStr]) -> Result<GLuint, String> {
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
        if PROGRAM_CACHE.get().is_some() && gl::ProgramParameteri::is_loaded() {
            gl::ProgramParameteri(
                program,
                gl::PROGRAM_BINARY_RETRIEVABLE_HINT,
                gl::TRUE as i32,
            );
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

#[cfg(test)]
mod shadow_tests {
    use super::*;

    #[test]
    fn compact_mask_matches_individual_texels() {
        for (width, height) in [(1, 1), (2, 17), (65, 64), (314, 199), (1876, 279)] {
            for scale in [0.75, 1., 2.] {
                let quad = Rect::new(0., 0., width as f32 / scale, height as f32 / scale);
                for (radius, blur) in [(0., 1.), (19.2, 22.), (60., 34.)] {
                    let mask = shadow_mask(quad, radius, blur, scale, width, height);
                    let hx = quad.w / 2. - blur;
                    let hy = quad.h / 2. - blur;
                    let r = radius.min(hx.min(hy));
                    for (index, &alpha) in mask.iter().enumerate() {
                        let x = index as u32 % width;
                        let y = index as u32 / width;
                        let x = -quad.w / 2. + (x.min(width - 1 - x) as f32 + 0.5) / scale;
                        let y = -quad.h / 2. + (y.min(height - 1 - y) as f32 + 0.5) / scale;
                        let (qx, qy) = (x.abs() - hx + r, y.abs() - hy + r);
                        let (ox, oy) = (qx.max(0.), qy.max(0.));
                        let distance = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.) - r;
                        let t = ((distance + blur) / (2. * blur)).clamp(0., 1.);
                        assert_eq!(alpha, ((1. - t * t * (3. - 2. * t)) * 255.).round() as u8);
                    }
                }
            }
        }
    }
}
