//! Lazy TTF metrics and individual outlines; never preprocess an entire font.
use geometry::lyon_tessellation::{
    FillRule,
    math::point,
    path::{Path, path::Builder},
};
use luma_vector_geometry::{self as geometry, Vertex};
use owned_ttf_parser::{AsFaceRef, GlyphId, OutlineBuilder, OwnedFace};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{
        DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target,
    },
};

/// Zero keeps original outlines. Bound hinting to small/medium text and valid densities.
pub fn hint_size(enabled: bool, physical_size: f32) -> u32 {
    if enabled && physical_size.is_finite() && (1.0..=64.0).contains(&physical_size) {
        physical_size.to_bits()
    } else {
        0
    }
}

pub struct Font {
    face: OwnedFace,
    // Most-recently-used sizes; no full-font meshes or bitmap atlas.
    hinters: Vec<(u32, HintingInstance)>,
}

pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
}

pub struct Metrics {
    pub advance_width: f32,
}

impl Font {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        OwnedFace::from_vec(bytes, 0)
            .map(|face| Self {
                face,
                hinters: Vec::new(),
            })
            .map_err(|e| e.to_string())
    }

    pub fn lookup_glyph_index(&self, character: char) -> u16 {
        self.face
            .as_face_ref()
            .glyph_index(character)
            .map_or(0, |id| id.0)
    }

    pub fn horizontal_line_metrics(&self, size: f32) -> Option<LineMetrics> {
        let face = self.face.as_face_ref();
        let scale = size / f32::from(face.units_per_em());
        Some(LineMetrics {
            ascent: f32::from(face.ascender()) * scale,
            descent: f32::from(face.descender()) * scale,
        })
    }

    pub fn metrics(&self, character: char, size: f32) -> Metrics {
        let face = self.face.as_face_ref();
        let glyph = GlyphId(self.lookup_glyph_index(character));
        Metrics {
            advance_width: f32::from(face.glyph_hor_advance(glyph).unwrap_or(0)) * size
                / f32::from(face.units_per_em()),
        }
    }

    pub fn horizontal_kern(&self, left: char, right: char, size: f32) -> Option<f32> {
        let face = self.face.as_face_ref();
        let left = face.glyph_index(left)?;
        let right = face.glyph_index(right)?;
        for table in face.tables().kern?.subtables {
            if table.horizontal
                && !table.has_cross_stream
                && let Some(value) = table.glyphs_kerning(left, right)
            {
                return Some(f32::from(value) * size / f32::from(face.units_per_em()));
            }
        }
        None
    }

    pub fn geometry(&mut self, character: char, hint_size: u32) -> Result<Vec<Vertex>, String> {
        if hint_size != 0 {
            return self.hinted_geometry(character, hint_size);
        }
        self.original_geometry(character)
    }

    fn hinted_geometry(&mut self, character: char, hint_size: u32) -> Result<Vec<Vertex>, String> {
        let ppem = f32::from_bits(hint_size);
        if self::hint_size(true, ppem) == 0 {
            return Err("Invalid hinting size".into());
        }
        let glyph_id = self.lookup_glyph_index(character);
        let font = FontRef::new(self.face.as_slice()).map_err(|e| e.to_string())?;
        let outlines = font.outline_glyphs();
        let Some(glyph) = outlines.get(skrifa::GlyphId::new(u32::from(glyph_id))) else {
            return Ok(Vec::new());
        };
        let hinter =
            if let Some(index) = self.hinters.iter().position(|(size, _)| *size == hint_size) {
                self.hinters.remove(index)
            } else {
                // Preserve layout/kerning and the font's horizontal proportions. The
                // automatic hinter aligns baseline, x-height and horizontal strokes.
                // Inter's prep stub contains no usable glyph hints, so AutoFallback
                // would select an interpreter that barely changes the outlines.
                let target = Target::Smooth {
                    mode: SmoothMode::Normal,
                    symmetric_rendering: false,
                    preserve_linear_metrics: true,
                };
                (
                    hint_size,
                    HintingInstance::new(
                        &outlines,
                        Size::new(ppem),
                        LocationRef::default(),
                        HintingOptions {
                            engine: if outlines.require_interpreter() {
                                Engine::Interpreter
                            } else {
                                Engine::Auto(None)
                            },
                            target,
                        },
                    )
                    .map_err(|e| e.to_string())?,
                )
            };
        if self.hinters.len() >= 8 {
            self.hinters.remove(0);
        }
        self.hinters.push(hinter);
        let mut outline = GlyphOutline {
            builder: Path::builder(),
            scale: 1. / ppem,
            open: false,
        };
        glyph
            .draw(
                DrawSettings::hinted(&self.hinters.last().unwrap().1, false),
                &mut outline,
            )
            .map_err(|e| e.to_string())?;
        OutlineBuilder::close(&mut outline);
        geometry::tessellate(&outline.builder.build(), 0.15 / 256., FillRule::NonZero)
    }

    fn original_geometry(&self, character: char) -> Result<Vec<Vertex>, String> {
        let face = self.face.as_face_ref();
        let mut outline = GlyphOutline {
            builder: Path::builder(),
            scale: 1. / f32::from(face.units_per_em()),
            open: false,
        };
        let glyph = GlyphId(self.lookup_glyph_index(character));
        if face.outline_glyph(glyph, &mut outline).is_none() {
            return Ok(Vec::new());
        }
        if outline.open {
            outline.builder.end(true);
        }
        // Flatten at subpixel precision for glyphs up to 256 physical px/em,
        // covering the current 1080p and 4K typography with the same geometry.
        geometry::tessellate(&outline.builder.build(), 0.15 / 256., FillRule::NonZero)
    }
}

struct GlyphOutline {
    builder: Builder,
    scale: f32,
    open: bool,
}

impl OutlineBuilder for GlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.open {
            self.builder.end(true);
        }
        self.builder.begin(point(x * self.scale, -y * self.scale));
        self.open = true;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(point(x * self.scale, -y * self.scale));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quadratic_bezier_to(
            point(x1 * self.scale, -y1 * self.scale),
            point(x * self.scale, -y * self.scale),
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_bezier_to(
            point(x1 * self.scale, -y1 * self.scale),
            point(x2 * self.scale, -y2 * self.scale),
            point(x * self.scale, -y * self.scale),
        );
    }
    fn close(&mut self) {
        if self.open {
            self.builder.end(true);
            self.open = false;
        }
    }
}

impl OutlinePen for GlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        OutlineBuilder::move_to(self, x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        OutlineBuilder::line_to(self, x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        OutlineBuilder::quad_to(self, x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        OutlineBuilder::curve_to(self, x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        OutlineBuilder::close(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Font {
        Font::from_bytes(include_bytes!("../tests/fixtures/inter-hinting.ttf").to_vec()).unwrap()
    }

    #[test]
    fn hinting_changes_real_outlines_and_tracks_physical_size_without_changing_layout() {
        let mut font = fixture();
        let width = font.metrics('H', 15.5).advance_width;
        let original = font.geometry('H', 0).unwrap();
        let hinted = font.geometry('H', hint_size(true, 15.5)).unwrap();
        assert!(!hinted.is_empty());
        // H's baseline and cap height must be fitted to physical pixels. A
        // prep-only no-op interpreter merely changes coordinate quantization.
        let top = |vertices: &[Vertex]| {
            vertices
                .iter()
                .map(|v| v.position.y * 15.5)
                .fold(0_f32, f32::min)
        };
        assert!((top(&hinted) - top(&hinted).round()).abs() < 0.001);
        assert!((top(&hinted) - top(&original)).abs() > 0.25);
        let positions = |vertices: &[Vertex]| {
            vertices
                .iter()
                .map(|v| [v.position.x.to_bits(), v.position.y.to_bits()])
                .collect::<Vec<_>>()
        };
        assert_ne!(positions(&original), positions(&hinted));
        let double = font.geometry('H', hint_size(true, 31.)).unwrap();
        assert_ne!(positions(&hinted), positions(&double));
        assert_eq!(width, font.metrics('H', 15.5).advance_width);
        assert_eq!(
            positions(&hinted),
            positions(&font.geometry('H', hint_size(true, 15.5)).unwrap())
        );
        for character in "HInmxäé ABCgjpq".chars() {
            for size in [12., 15.5, 20.8, 32., 64.] {
                let geometry = font.geometry(character, hint_size(true, size)).unwrap();
                assert!(
                    geometry
                        .iter()
                        .all(|v| v.position.x.is_finite() && v.position.y.is_finite())
                );
            }
        }
        for size in 10..40 {
            font.geometry('H', hint_size(true, size as f32)).unwrap();
        }
        assert!(font.hinters.len() <= 8);
    }

    #[test]
    fn hint_size_rejects_invalid_sizes_and_leaves_large_text_original() {
        for size in [f32::NAN, f32::INFINITY, 0., -2., 65.] {
            assert_eq!(hint_size(true, size), 0);
        }
        assert_eq!(hint_size(false, 16.), 0);
        assert_ne!(hint_size(true, 16.), hint_size(true, 32.));
    }
}
