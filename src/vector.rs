//! Precompiled SVG triangle geometry, uploaded once without image textures.
use gl::types::{GLint, GLuint};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, path::Path};

/// Whether meshes get their own vertex array object (GLES 3 or desktop core). The renderer
/// decides once its context exists.
static VERTEX_ARRAYS: AtomicBool = AtomicBool::new(false);

pub fn use_vertex_arrays(enabled: bool) {
    VERTEX_ARRAYS.store(enabled, Ordering::Relaxed);
}

/// Points attributes 0-3 at a mesh buffer's interleaved position, offset, colour/origin and
/// coverage, as the vector and text programs read them; attribute 4 stays off.
pub fn bind_attributes(buffer: GLuint) {
    unsafe {
        gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
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
    }
}

pub struct Mesh {
    pub buffer: GLuint,
    /// The buffer's attribute setup, recorded once (0 without vertex array objects):
    /// one bind instead of about ten calls per draw, each costly in TV drivers.
    pub vertex_array: GLuint,
    pub width: f32,
    pub height: f32,
    pub vertices: i32,
    pub bytes: usize,
}

impl Mesh {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes =
            fs::read(path).map_err(|e| format!("Vector artwork {}: {e}", path.display()))?;
        if bytes.len() < 20 || &bytes[..8] != b"LUMAVEC1" {
            return Err(format!("Invalid vector artwork header: {}", path.display()));
        }
        let float = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let width = float(8);
        let height = float(12);
        let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.
            || height <= 0.
            || count == 0
            || count > 1_000_000
            || !count.is_multiple_of(3)
            || bytes.len() != 20 + count * 36
        {
            return Err(format!(
                "Invalid vector artwork dimensions: {}",
                path.display()
            ));
        }
        let vertices: Vec<f32> = bytes[20..]
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        if vertices.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "Invalid vector artwork coordinates: {}",
                path.display()
            ));
        }
        Self::upload(width, height, &vertices)
    }

    pub fn upload(width: f32, height: f32, vertices: &[f32]) -> Result<Self, String> {
        let count = vertices.len() / 9;
        if count == 0 || !count.is_multiple_of(3) || vertices.len() != count * 9 {
            return Err("Invalid vector buffer vertex count".into());
        }
        if vertices.iter().any(|value| !value.is_finite()) {
            return Err("Invalid vector buffer coordinates".into());
        }
        let mut buffer = 0;
        unsafe {
            gl::GenBuffers(1, &mut buffer);
            gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                std::mem::size_of_val(vertices) as isize,
                vertices.as_ptr().cast(),
                gl::STATIC_DRAW,
            );
        }
        let mut vertex_array = 0;
        if VERTEX_ARRAYS.load(Ordering::Relaxed) {
            unsafe {
                let mut previous: GLint = 0;
                gl::GetIntegerv(gl::VERTEX_ARRAY_BINDING, &mut previous);
                gl::GenVertexArrays(1, &mut vertex_array);
                gl::BindVertexArray(vertex_array);
                bind_attributes(buffer);
                gl::BindVertexArray(previous as GLuint);
            }
        }
        Ok(Self {
            buffer,
            vertex_array,
            width,
            height,
            vertices: count as i32,
            bytes: count * 36,
        })
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        unsafe {
            if self.vertex_array != 0 {
                gl::DeleteVertexArrays(1, &self.vertex_array);
            }
            gl::DeleteBuffers(1, &self.buffer);
        }
    }
}
