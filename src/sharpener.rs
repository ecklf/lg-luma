//! Optional bounded-contrast whole-surface sharpening. No display-mode changes.
use super::{GraphicsInfo, Texture, link, texture};
use gl::types::*;

pub struct Sharpener {
    program: GLuint,
    buffer: GLuint,
    framebuffer: GLuint,
    image: Option<Texture>,
}

impl Sharpener {
    pub fn new(info: &GraphicsInfo) -> Result<Self, String> {
        if !gl::BlitFramebuffer::is_loaded() || info.version.starts_with("OpenGL ES 2") {
            return Err("requires framebuffer blit (GLES 3 / desktop GL 3)".into());
        }
        if !info.fragment_highp {
            return Err("requires high-precision fragment coordinates".into());
        }
        let gles = info.version.starts_with("OpenGL ES");
        let vertex_prefix = if gles {
            "#version 100\nprecision highp float;\n#define PAINT_PRECISION highp\n"
        } else {
            "#version 150\n#define attribute in\n#define varying out\n#define PAINT_PRECISION\n"
        };
        let fragment_prefix = if gles {
            "#version 100\nprecision highp float;\n#define PAINT_PRECISION highp\n"
        } else {
            "#version 150\n#define varying in\n#define texture2D texture\nout vec4 out_color;\n#define gl_FragColor out_color\n#define PAINT_PRECISION\n"
        };
        let program = link(
            &(vertex_prefix.to_owned() + include_str!("shaders/sharpen.vert")),
            &(fragment_prefix.to_owned() + include_str!("shaders/sharpen.frag")),
            &[c"a_position"],
        )?;
        let mut buffer = 0;
        let mut framebuffer = 0;
        let vertices: [f32; 12] = [-1., -1., 1., -1., -1., 1., -1., 1., 1., -1., 1., 1.];
        unsafe {
            gl::GenBuffers(1, &mut buffer);
            gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                size_of_val(&vertices) as isize,
                vertices.as_ptr().cast(),
                gl::STATIC_DRAW,
            );
            gl::GenFramebuffers(1, &mut framebuffer);
        }
        Ok(Self {
            program,
            buffer,
            framebuffer,
            image: None,
        })
    }

    pub fn apply(&mut self, width: u32, height: u32, strength: f32) -> Result<usize, String> {
        let mut target = 0;
        unsafe {
            gl::GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut target);
        }
        let target = target as GLuint;
        let result = self.apply_to(target, width, height, strength);
        // Restore the caller's framebuffer even when allocation/blit fails.
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, target);
        }
        result
    }

    fn apply_to(
        &mut self,
        target: GLuint,
        width: u32,
        height: u32,
        strength: f32,
    ) -> Result<usize, String> {
        if width == 0 || height == 0 {
            return Err("empty drawable".into());
        }
        unsafe {
            if self
                .image
                .as_ref()
                .is_none_or(|image| image.width != width || image.height != height)
            {
                self.image = Some(texture(width, height, &[]));
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            }
            let image = self.image.as_ref().unwrap();
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.framebuffer);
            gl::FramebufferTexture2D(
                gl::DRAW_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                image.id,
                0,
            );
            if gl::CheckFramebufferStatus(gl::DRAW_FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                return Err("incomplete sharpening framebuffer".into());
            }
            gl::Disable(gl::SCISSOR_TEST);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, target);
            // Same-size resolve preserves window MSAA; do not re-render as single-sample.
            gl::BlitFramebuffer(
                0,
                0,
                width as i32,
                height as i32,
                0,
                0,
                width as i32,
                height as i32,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                return Err(format!("frame resolve GL error 0x{error:x}"));
            }
        }
        self.draw_cached(target, width, height, strength)
    }

    /// Reuse the resolved source when the UI has not changed. No repeated resolve.
    pub fn draw_cached(
        &self,
        target: GLuint,
        width: u32,
        height: u32,
        strength: f32,
    ) -> Result<usize, String> {
        let image = self.image.as_ref().ok_or("No resolved clarity frame")?;
        if image.width != width || image.height != height {
            return Err("Clarity size changed".into());
        }
        unsafe {
            gl::Disable(gl::SCISSOR_TEST);
            gl::BindFramebuffer(gl::FRAMEBUFFER, target);
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::Disable(gl::BLEND);
            gl::UseProgram(self.program);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.buffer);
            for index in 1..5 {
                gl::DisableVertexAttribArray(index);
            }
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 8, std::ptr::null());
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, image.id);
            gl::Uniform1i(gl::GetUniformLocation(self.program, c"u_image".as_ptr()), 0);
            gl::Uniform2f(
                gl::GetUniformLocation(self.program, c"u_texel".as_ptr()),
                1. / width as f32,
                1. / height as f32,
            );
            gl::Uniform1f(
                gl::GetUniformLocation(self.program, c"u_strength".as_ptr()),
                strength,
            );
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::Enable(gl::BLEND);
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                return Err(format!("sharpening draw GL error 0x{error:x}"));
            }
        }
        Ok(width as usize * height as usize * 4)
    }
}

impl Drop for Sharpener {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.program);
            gl::DeleteBuffers(1, &self.buffer);
            gl::DeleteFramebuffers(1, &self.framebuffer);
        }
    }
}
