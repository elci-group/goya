//! The GPU compositor (feature `gpu`): EGL and OpenGL ES 2.0, loaded at run
//! time.
//!
//! The scene's display list ([`crate::DrawOp`]) is drawn every frame:
//! rect fills as quads whose edge coverage is computed exactly in the
//! fragment shader, and graphics, strokes, text and the focus ring as
//! textures that tiny-skia rasterized once ([`goya_scene_runtime::Scene::sprite`])
//! and that stay on the GPU while their [`SpriteKey`] is in use. A breathing
//! background or a fading card therefore costs one uniform, and a focus
//! move or a card flip costs one small raster the first time it is seen.
//!
//! Nothing is linked: `libEGL` and `libGLESv2` are opened with `dlopen`, so
//! the same binary falls back to software rendering where they are missing.
//! The TV's userland uses the soft-float calling convention, which is what
//! `armv7-unknown-linux-gnueabi` uses (`scripts/viuda-armel-sysroot.sh`).

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::time::{Duration, Instant};

use goya_scene_runtime::{DrawOp, Scene};

use crate::{Pixmap, SpriteKey};

type Ptr = *mut c_void;

/// A library opened with `dlopen`, kept open for the process.
struct Lib(Ptr);

impl Lib {
    fn open(names: &[&str]) -> Result<Lib, String> {
        for name in names {
            let c = CString::new(*name).map_err(|e| e.to_string())?;
            // SAFETY: `c` is a NUL-terminated name; dlopen has no other
            // preconditions. The handle is never closed.
            let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
            if !h.is_null() {
                return Ok(Lib(h));
            }
        }
        Err(format!("cannot load {}", names.join(" or ")))
    }

    fn sym(&self, name: &CStr) -> Ptr {
        // SAFETY: `self.0` is a live dlopen handle and `name` is
        // NUL-terminated.
        unsafe { libc::dlsym(self.0, name.as_ptr()) }
    }
}

/// A table of C function pointers, each looked up by its own name.
macro_rules! api {
    ($table:ident { $( fn $f:ident ( $( $a:ty ),* ) $( -> $r:ty )? ; )* }) => {
        #[allow(non_snake_case)]
        struct $table { $( $f: unsafe extern "C" fn( $( $a ),* ) $( -> $r )?, )* }
        impl $table {
            fn load(get: &dyn Fn(&CStr) -> Ptr) -> Result<$table, String> {
                Ok($table { $( $f: {
                    let p = get(match CStr::from_bytes_with_nul(concat!(stringify!($f), "\0").as_bytes()) {
                        Ok(c) => c,
                        Err(e) => return Err(e.to_string()),
                    });
                    if p.is_null() {
                        return Err(format!("missing {}", stringify!($f)));
                    }
                    // SAFETY: `p` is the address of the C function of this
                    // name, whose signature the Khronos headers give as
                    // declared here.
                    unsafe { std::mem::transmute::<Ptr, unsafe extern "C" fn( $( $a ),* ) $( -> $r )?>(p) }
                }, )* })
            }
        }
    };
}

api!(Egl {
    fn eglGetDisplay(Ptr) -> Ptr;
    fn eglInitialize(Ptr, *mut i32, *mut i32) -> u32;
    fn eglBindAPI(u32) -> u32;
    fn eglChooseConfig(Ptr, *const i32, *mut Ptr, i32, *mut i32) -> u32;
    fn eglCreateContext(Ptr, Ptr, Ptr, *const i32) -> Ptr;
    fn eglCreateWindowSurface(Ptr, Ptr, Ptr, *const i32) -> Ptr;
    fn eglCreatePbufferSurface(Ptr, Ptr, *const i32) -> Ptr;
    fn eglMakeCurrent(Ptr, Ptr, Ptr, Ptr) -> u32;
    fn eglSwapBuffers(Ptr, Ptr) -> u32;
    fn eglSwapInterval(Ptr, i32) -> u32;
    fn eglGetError() -> i32;
    fn eglQueryString(Ptr, i32) -> *const c_char;
    fn eglGetProcAddress(*const c_char) -> Ptr;
});

api!(Gl {
    fn glCreateShader(u32) -> u32;
    fn glShaderSource(u32, i32, *const *const c_char, *const i32);
    fn glCompileShader(u32);
    fn glGetShaderiv(u32, u32, *mut i32);
    fn glGetShaderInfoLog(u32, i32, *mut i32, *mut c_char);
    fn glCreateProgram() -> u32;
    fn glAttachShader(u32, u32);
    fn glBindAttribLocation(u32, u32, *const c_char);
    fn glLinkProgram(u32);
    fn glGetProgramiv(u32, u32, *mut i32);
    fn glGetProgramInfoLog(u32, i32, *mut i32, *mut c_char);
    fn glUseProgram(u32);
    fn glGetUniformLocation(u32, *const c_char) -> i32;
    fn glUniform1f(i32, f32);
    fn glUniform1i(i32, i32);
    fn glUniform2f(i32, f32, f32);
    fn glUniform4f(i32, f32, f32, f32, f32);
    fn glGenBuffers(i32, *mut u32);
    fn glBindBuffer(u32, u32);
    fn glBufferData(u32, isize, *const c_void, u32);
    fn glVertexAttribPointer(u32, i32, u32, u8, i32, *const c_void);
    fn glEnableVertexAttribArray(u32);
    fn glGenTextures(i32, *mut u32);
    fn glDeleteTextures(i32, *const u32);
    fn glBindTexture(u32, u32);
    fn glActiveTexture(u32);
    fn glTexParameteri(u32, u32, i32);
    fn glTexImage2D(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
    fn glPixelStorei(u32, i32);
    fn glGenFramebuffers(i32, *mut u32);
    fn glBindFramebuffer(u32, u32);
    fn glFramebufferTexture2D(u32, u32, u32, u32, i32);
    fn glCheckFramebufferStatus(u32) -> u32;
    fn glViewport(i32, i32, i32, i32);
    fn glClearColor(f32, f32, f32, f32);
    fn glClear(u32);
    fn glEnable(u32);
    fn glDisable(u32);
    fn glBlendFunc(u32, u32);
    fn glDrawArrays(u32, i32, i32);
    fn glReadPixels(i32, i32, i32, i32, u32, u32, *mut c_void);
    fn glFinish();
    fn glGetError() -> u32;
    fn glGetString(u32) -> *const u8;
});

// EGL.
const EGL_ALPHA_SIZE: i32 = 0x3021;
const EGL_BLUE_SIZE: i32 = 0x3022;
const EGL_GREEN_SIZE: i32 = 0x3023;
const EGL_RED_SIZE: i32 = 0x3024;
const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_NONE: i32 = 0x3038;
const EGL_RENDERABLE_TYPE: i32 = 0x3040;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_WIDTH: i32 = 0x3057;
const EGL_VENDOR: i32 = 0x3053;
const EGL_VERSION: i32 = 0x3054;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_PBUFFER_BIT: i32 = 0x0001;
const EGL_WINDOW_BIT: i32 = 0x0004;
const EGL_OPENGL_ES2_BIT: i32 = 0x0004;
const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;

// OpenGL ES 2.0.
const GL_TRIANGLE_STRIP: u32 = 0x0005;
const GL_ONE: u32 = 1;
const GL_ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
const GL_BLEND: u32 = 0x0BE2;
const GL_DITHER: u32 = 0x0BD0;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FLOAT: u32 = 0x1406;
const GL_RGBA: u32 = 0x1908;
const GL_VENDOR: u32 = 0x1F00;
const GL_RENDERER: u32 = 0x1F01;
const GL_VERSION: u32 = 0x1F02;
const GL_NEAREST: i32 = 0x2600;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_ARRAY_BUFFER: u32 = 0x8892;
const GL_STATIC_DRAW: u32 = 0x88E4;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_COMPILE_STATUS: u32 = 0x8B81;
const GL_LINK_STATUS: u32 = 0x8B82;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER: u32 = 0x8D40;

/// Every quad is the unit square, placed by `u_box` (pixels, top-left
/// origin) in a viewport of `u_size` pixels.
const VERTEX: &str = "
attribute vec2 a_unit;
uniform vec4 u_box;
uniform vec2 u_size;
varying vec2 v_uv;
void main() {
    vec2 p = mix(u_box.xy, u_box.zw, a_unit);
    v_uv = a_unit;
    gl_Position = vec4(p.x / u_size.x * 2.0 - 1.0, 1.0 - p.y / u_size.y * 2.0, 0.0, 1.0);
}";

/// Pixel coordinates reach 1920, beyond `mediump`'s exact integers.
const PRECISION: &str = "
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
";

/// A rect fill: the exact area of the pixel inside `u_rect`, times the
/// premultiplied colour.
const RECT: &str = "
uniform vec4 u_rect;
uniform vec4 u_colour;
uniform float u_height;
void main() {
    vec2 px = vec2(floor(gl_FragCoord.x), u_height - 1.0 - floor(gl_FragCoord.y));
    vec2 c = clamp(min(px + 1.0, u_rect.zw) - max(px, u_rect.xy), 0.0, 1.0);
    gl_FragColor = u_colour * (c.x * c.y);
}";

/// A sprite: one texel per pixel (nearest, aligned), times its opacity.
const SPRITE: &str = "
uniform sampler2D u_tex;
uniform float u_alpha;
varying vec2 v_uv;
void main() {
    gl_FragColor = texture2D(u_tex, v_uv) * u_alpha;
}";

struct Program {
    id: u32,
    box_: i32,
    size: i32,
    a: i32,
    b: i32,
    c: i32,
}

/// One sprite on the GPU. `tex` is 0 for a sprite that paints nothing.
struct Texture {
    tex: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    bytes: usize,
    used: u64,
}

/// What one frame cost.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameStats {
    pub ops: usize,
    /// Sprites rasterized and uploaded in this frame.
    pub uploads: usize,
    pub upload_bytes: usize,
    /// CPU time spent rasterizing those sprites.
    pub raster: Duration,
    /// CPU time spent issuing GL calls (not GPU time).
    pub submit: Duration,
}

/// Where the compositor draws.
enum Target {
    /// An on-screen EGL window surface.
    Window(Ptr),
    /// An offscreen texture, for headless runs and tests.
    Offscreen { fbo: u32 },
}

/// An EGL context drawing display lists.
pub struct Gpu {
    egl: Egl,
    gl: Gl,
    display: Ptr,
    target: Target,
    width: i32,
    height: i32,
    rect: Program,
    sprite: Program,
    textures: HashMap<SpriteKey, Texture>,
    resident: usize,
    frame: u64,
    /// EGL and GL vendor, renderer and version.
    pub info: String,
}

/// Textures kept beyond this many bytes are dropped when no longer drawn.
const BUDGET: usize = 48 << 20;

fn libs() -> Result<(Egl, Gl), String> {
    let egl_lib = Lib::open(&["libEGL.so.1", "libEGL.so"])?;
    let gl_lib = Lib::open(&["libGLESv2.so.2", "libGLESv2.so"])?;
    let egl = Egl::load(&|n| egl_lib.sym(n))?;
    let gl = Gl::load(&|n| {
        let p = gl_lib.sym(n);
        if p.is_null() {
            // SAFETY: `n` is NUL-terminated; eglGetProcAddress only reads it.
            unsafe { (egl.eglGetProcAddress)(n.as_ptr()) }
        } else {
            p
        }
    })?;
    // `Lib` never closes its handle, so the functions stay loaded.
    Ok((egl, gl))
}

fn egl_string(egl: &Egl, display: Ptr, name: i32) -> String {
    // SAFETY: an initialized display; the result is NULL or a static
    // NUL-terminated string.
    let p = unsafe { (egl.eglQueryString)(display, name) };
    if p.is_null() {
        return "?".into();
    }
    // SAFETY: as above.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

impl Gpu {
    /// A compositor drawing to a Wayland surface.
    ///
    /// # Safety
    ///
    /// `display` must be a live C `wl_display*` and `window` a live
    /// `wl_egl_window*` on it, both outliving the returned compositor.
    pub unsafe fn window(display: Ptr, window: Ptr, width: u32, height: u32) -> Result<Gpu, String> {
        let (egl, gl) = libs()?;
        // SAFETY: `display` is a live wl_display, which EGL on the Wayland
        // platform takes as its native display.
        let dpy = unsafe { (egl.eglGetDisplay)(display) };
        Self::create(egl, gl, dpy, width, height, Some(window))
    }

    /// A compositor drawing offscreen at `width`×`height`, without a
    /// display server where EGL allows it (Mesa's surfaceless platform).
    pub fn headless(width: u32, height: u32) -> Result<Gpu, String> {
        let (egl, gl) = libs()?;
        // EGL 1.5's eglGetPlatformDisplay, when there is one.
        let name = c"eglGetPlatformDisplay";
        // SAFETY: a NUL-terminated name.
        let get = unsafe { (egl.eglGetProcAddress)(name.as_ptr()) };
        let dpy = if get.is_null() {
            std::ptr::null_mut()
        } else {
            // SAFETY: eglGetPlatformDisplay's signature per EGL 1.5.
            let f: unsafe extern "C" fn(u32, Ptr, *const isize) -> Ptr = unsafe { std::mem::transmute(get) };
            let attrs = [EGL_NONE as isize];
            // SAFETY: a valid platform, a NULL native display, a
            // terminated attribute list.
            unsafe { f(EGL_PLATFORM_SURFACELESS_MESA, std::ptr::null_mut(), attrs.as_ptr()) }
        };
        let dpy = if dpy.is_null() {
            // SAFETY: EGL_DEFAULT_DISPLAY.
            unsafe { (egl.eglGetDisplay)(std::ptr::null_mut()) }
        } else {
            dpy
        };
        Self::create(egl, gl, dpy, width, height, None)
    }

    fn create(egl: Egl, gl: Gl, dpy: Ptr, width: u32, height: u32, window: Option<Ptr>) -> Result<Gpu, String> {
        let fail = |what: &str| {
            // SAFETY: no preconditions.
            format!("GY-VIUDA-GPU: {what} failed (EGL error 0x{:x})", unsafe { (egl.eglGetError)() })
        };
        if dpy.is_null() {
            return Err(fail("eglGetDisplay"));
        }
        let (mut major, mut minor) = (0, 0);
        // SAFETY: `dpy` is a display handle; the out-pointers are valid.
        if unsafe { (egl.eglInitialize)(dpy, &mut major, &mut minor) } == 0 {
            return Err(fail("eglInitialize"));
        }
        // SAFETY: no preconditions.
        if unsafe { (egl.eglBindAPI)(EGL_OPENGL_ES_API) } == 0 {
            return Err(fail("eglBindAPI"));
        }
        let surface_bit = if window.is_some() { EGL_WINDOW_BIT } else { EGL_PBUFFER_BIT };
        let attrs = [
            EGL_SURFACE_TYPE, surface_bit,
            EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
            EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 0,
            EGL_NONE,
        ];
        let mut config: Ptr = std::ptr::null_mut();
        let mut n = 0;
        // SAFETY: a terminated attribute list and room for one config.
        if unsafe { (egl.eglChooseConfig)(dpy, attrs.as_ptr(), &mut config, 1, &mut n) } == 0 || n < 1 {
            return Err(fail("eglChooseConfig"));
        }
        let ctx_attrs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
        // SAFETY: valid display and config; no shared context.
        let ctx = unsafe { (egl.eglCreateContext)(dpy, config, std::ptr::null_mut(), ctx_attrs.as_ptr()) };
        if ctx.is_null() {
            return Err(fail("eglCreateContext"));
        }
        let surface = match window {
            // SAFETY: `w` is a live wl_egl_window, EGL's native window on
            // the Wayland platform.
            Some(w) => unsafe { (egl.eglCreateWindowSurface)(dpy, config, w, [EGL_NONE].as_ptr()) },
            None => {
                let pb = [EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE];
                // SAFETY: a terminated attribute list.
                unsafe { (egl.eglCreatePbufferSurface)(dpy, config, pb.as_ptr()) }
            }
        };
        if surface.is_null() {
            return Err(fail("eglCreate*Surface"));
        }
        // SAFETY: all three handles were created on `dpy`.
        if unsafe { (egl.eglMakeCurrent)(dpy, surface, surface, ctx) } == 0 {
            return Err(fail("eglMakeCurrent"));
        }
        let gl_string = |name| {
            // SAFETY: a current context; the result is NULL or static.
            let p = unsafe { (gl.glGetString)(name) };
            if p.is_null() {
                "?".to_owned()
            } else {
                // SAFETY: as above.
                unsafe { CStr::from_ptr(p.cast()) }.to_string_lossy().into_owned()
            }
        };
        let info = format!(
            "egl={} {} gl_vendor={} gl_renderer={} gl_version={}",
            egl_string(&egl, dpy, EGL_VENDOR),
            egl_string(&egl, dpy, EGL_VERSION),
            gl_string(GL_VENDOR),
            gl_string(GL_RENDERER),
            gl_string(GL_VERSION)
        );
        let target = match window {
            Some(_) => {
                // Wait for the compositor's frame callback before reusing a
                // buffer, so frames are paced by the display.
                // SAFETY: a current display.
                unsafe { (egl.eglSwapInterval)(dpy, 1) };
                Target::Window(surface)
            }
            None => {
                let (mut fbo, mut tex) = (0, 0);
                // SAFETY: a current context; out-pointers are valid.
                unsafe {
                    (gl.glGenTextures)(1, &mut tex);
                    (gl.glBindTexture)(GL_TEXTURE_2D, tex);
                    (gl.glTexImage2D)(GL_TEXTURE_2D, 0, GL_RGBA as i32, width as i32, height as i32, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
                    (gl.glGenFramebuffers)(1, &mut fbo);
                    (gl.glBindFramebuffer)(GL_FRAMEBUFFER, fbo);
                    (gl.glFramebufferTexture2D)(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
                    if (gl.glCheckFramebufferStatus)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                        return Err("GY-VIUDA-GPU: offscreen framebuffer incomplete".into());
                    }
                }
                Target::Offscreen { fbo }
            }
        };
        let rect = program(&gl, RECT, &["u_rect", "u_colour", "u_height"])?;
        let sprite = program(&gl, SPRITE, &["u_tex", "u_alpha", "u_alpha"])?;
        let unit: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let mut vbo = 0;
        // SAFETY: a current context; `unit` outlives the copy into the
        // buffer; attribute 0 is bound to `a_unit` in every program.
        unsafe {
            (gl.glGenBuffers)(1, &mut vbo);
            (gl.glBindBuffer)(GL_ARRAY_BUFFER, vbo);
            (gl.glBufferData)(GL_ARRAY_BUFFER, std::mem::size_of_val(&unit) as isize, unit.as_ptr().cast(), GL_STATIC_DRAW);
            (gl.glVertexAttribPointer)(0, 2, GL_FLOAT, 0, 0, std::ptr::null());
            (gl.glEnableVertexAttribArray)(0);
            (gl.glPixelStorei)(GL_UNPACK_ALIGNMENT, 1);
            (gl.glPixelStorei)(GL_PACK_ALIGNMENT, 1);
            (gl.glDisable)(GL_DITHER);
            (gl.glEnable)(GL_BLEND);
            (gl.glBlendFunc)(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
            (gl.glActiveTexture)(GL_TEXTURE0);
        }
        Ok(Gpu {
            egl,
            gl,
            display: dpy,
            target,
            width: width as i32,
            height: height as i32,
            rect,
            sprite,
            textures: HashMap::new(),
            resident: 0,
            frame: 0,
            info,
        })
    }

    /// Bytes of sprite textures held on the GPU.
    pub fn resident_bytes(&self) -> usize {
        self.resident
    }

    /// Draw `ops` from `scene` into the back buffer.
    pub fn draw(&mut self, scene: &Scene, ops: &[DrawOp]) -> FrameStats {
        let mut stats = FrameStats { ops: ops.len(), ..FrameStats::default() };
        self.frame += 1;
        let t = Instant::now();
        let gl = &self.gl;
        let (w, h) = (self.width as f32, self.height as f32);
        // SAFETY: the context is current on this thread for the life of
        // `self`; every name passed was created by it.
        unsafe {
            if let Target::Offscreen { fbo, .. } = self.target {
                (gl.glBindFramebuffer)(GL_FRAMEBUFFER, fbo);
            }
            (gl.glViewport)(0, 0, self.width, self.height);
            (gl.glClearColor)(1.0, 1.0, 1.0, 1.0);
            (gl.glClear)(GL_COLOR_BUFFER_BIT);
        }
        let mut current = 0;
        for op in ops {
            match op {
                DrawOp::Rect { rect, rgba } => {
                    let a = f32::from(rgba[3]) / 255.0;
                    let c = |v: u8| f32::from(v) / 255.0 * a;
                    let quad = [rect[0].floor(), rect[1].floor(), rect[2].ceil(), rect[3].ceil()];
                    // SAFETY: as above; the uniforms belong to `rect`.
                    unsafe {
                        if current != self.rect.id {
                            (gl.glUseProgram)(self.rect.id);
                            (gl.glUniform2f)(self.rect.size, w, h);
                            (gl.glUniform1f)(self.rect.c, h);
                            current = self.rect.id;
                        }
                        (gl.glUniform4f)(self.rect.box_, quad[0], quad[1], quad[2], quad[3]);
                        (gl.glUniform4f)(self.rect.a, rect[0], rect[1], rect[2], rect[3]);
                        (gl.glUniform4f)(self.rect.b, c(rgba[0]), c(rgba[1]), c(rgba[2]), a);
                        (gl.glDrawArrays)(GL_TRIANGLE_STRIP, 0, 4);
                    }
                }
                DrawOp::Sprite { key, alpha } => {
                    if !self.textures.contains_key(key) {
                        let r = Instant::now();
                        let made = scene.sprite(key);
                        stats.raster += r.elapsed();
                        let tex = match made {
                            None => Texture { tex: 0, x: 0, y: 0, w: 0, h: 0, bytes: 0, used: 0 },
                            Some(s) => {
                                let (sw, sh) = (s.pixmap.width() as i32, s.pixmap.height() as i32);
                                let mut id = 0;
                                // SAFETY: the pixmap holds sw*sh RGBA
                                // pixels, tightly packed.
                                unsafe {
                                    (gl.glGenTextures)(1, &mut id);
                                    (gl.glBindTexture)(GL_TEXTURE_2D, id);
                                    for (p, v) in [
                                        (GL_TEXTURE_MIN_FILTER, GL_NEAREST),
                                        (GL_TEXTURE_MAG_FILTER, GL_NEAREST),
                                        (GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE),
                                        (GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE),
                                    ] {
                                        (gl.glTexParameteri)(GL_TEXTURE_2D, p, v);
                                    }
                                    (gl.glTexImage2D)(GL_TEXTURE_2D, 0, GL_RGBA as i32, sw, sh, 0, GL_RGBA, GL_UNSIGNED_BYTE, s.pixmap.data().as_ptr().cast());
                                }
                                let bytes = s.pixmap.data().len();
                                stats.uploads += 1;
                                stats.upload_bytes += bytes;
                                self.resident += bytes;
                                Texture { tex: id, x: s.x, y: s.y, w: sw, h: sh, bytes, used: 0 }
                            }
                        };
                        self.textures.insert(key.clone(), tex);
                    }
                    let Some(t) = self.textures.get_mut(key) else { continue };
                    t.used = self.frame;
                    if t.tex == 0 || *alpha <= 0.0 {
                        continue;
                    }
                    // SAFETY: as above; the uniforms belong to `sprite`.
                    unsafe {
                        if current != self.sprite.id {
                            (gl.glUseProgram)(self.sprite.id);
                            (gl.glUniform2f)(self.sprite.size, w, h);
                            (gl.glUniform1i)(self.sprite.a, 0);
                            current = self.sprite.id;
                        }
                        (gl.glBindTexture)(GL_TEXTURE_2D, t.tex);
                        (gl.glUniform4f)(self.sprite.box_, t.x as f32, t.y as f32, (t.x + t.w) as f32, (t.y + t.h) as f32);
                        (gl.glUniform1f)(self.sprite.b, *alpha);
                        (gl.glDrawArrays)(GL_TRIANGLE_STRIP, 0, 4);
                    }
                }
            }
        }
        self.evict();
        stats.submit = t.elapsed().saturating_sub(stats.raster);
        stats
    }

    /// Drop sprites not drawn in this frame, oldest first, while over
    /// [`BUDGET`].
    fn evict(&mut self) {
        if self.resident <= BUDGET {
            return;
        }
        let mut idle: Vec<(u64, SpriteKey)> = self
            .textures
            .iter()
            .filter(|(_, t)| t.used < self.frame)
            .map(|(k, t)| (t.used, k.clone()))
            .collect();
        idle.sort_by_key(|(used, _)| *used);
        for (_, key) in idle {
            if self.resident <= BUDGET {
                break;
            }
            if let Some(t) = self.textures.remove(&key) {
                // SAFETY: a texture this context created.
                unsafe { (self.gl.glDeleteTextures)(1, &t.tex) };
                self.resident -= t.bytes;
            }
        }
    }

    /// Show the frame: swap buffers on a window, finish offscreen.
    pub fn present(&mut self) -> Result<(), String> {
        match self.target {
            Target::Window(surface) => {
                // SAFETY: the current display and its window surface.
                if unsafe { (self.egl.eglSwapBuffers)(self.display, surface) } == 0 {
                    // SAFETY: no preconditions.
                    return Err(format!("GY-VIUDA-GPU: eglSwapBuffers failed (EGL error 0x{:x})", unsafe { (self.egl.eglGetError)() }));
                }
            }
            // SAFETY: a current context.
            Target::Offscreen { .. } => unsafe { (self.gl.glFinish)() },
        }
        // SAFETY: a current context.
        match unsafe { (self.gl.glGetError)() } {
            0 => Ok(()),
            e => Err(format!("GY-VIUDA-GPU: GL error 0x{e:x}")),
        }
    }

    /// Read the offscreen frame back, top row first. `None` on a window.
    pub fn read(&mut self) -> Option<Pixmap> {
        let Target::Offscreen { fbo } = self.target else { return None };
        let mut pm = Pixmap::new(self.width as u32, self.height as u32)?;
        let row = self.width as usize * 4;
        let mut flipped = vec![0u8; row * self.height as usize];
        // SAFETY: a current context; `flipped` holds width*height RGBA
        // pixels.
        unsafe {
            (self.gl.glBindFramebuffer)(GL_FRAMEBUFFER, fbo);
            (self.gl.glReadPixels)(0, 0, self.width, self.height, GL_RGBA, GL_UNSIGNED_BYTE, flipped.as_mut_ptr().cast());
        }
        let data = pm.data_mut();
        for (y, src) in flipped.chunks(row).enumerate() {
            let at = (self.height as usize - 1 - y) * row;
            data[at..at + row].copy_from_slice(src);
        }
        Some(pm)
    }
}

/// Compile the shared vertex shader with `fragment`, bind `a_unit` to
/// attribute 0, and look up `u_box`, `u_size` and three more uniforms.
fn program(gl: &Gl, fragment: &str, uniforms: &[&str; 3]) -> Result<Program, String> {
    let shader = |kind, src: &str| -> Result<u32, String> {
        let text = if kind == GL_FRAGMENT_SHADER { format!("{PRECISION}{src}") } else { src.to_owned() };
        let c = CString::new(text).map_err(|e| e.to_string())?;
        // SAFETY: a current context; one NUL-terminated source string.
        unsafe {
            let id = (gl.glCreateShader)(kind);
            (gl.glShaderSource)(id, 1, &c.as_ptr(), std::ptr::null());
            (gl.glCompileShader)(id);
            let mut ok = 0;
            (gl.glGetShaderiv)(id, GL_COMPILE_STATUS, &mut ok);
            if ok == 0 {
                let mut log = vec![0u8; 1024];
                let mut n = 0;
                (gl.glGetShaderInfoLog)(id, log.len() as i32, &mut n, log.as_mut_ptr().cast());
                return Err(format!("GY-VIUDA-GPU: shader: {}", String::from_utf8_lossy(&log[..n.max(0) as usize])));
            }
            Ok(id)
        }
    };
    let (vs, fs) = (shader(GL_VERTEX_SHADER, VERTEX)?, shader(GL_FRAGMENT_SHADER, fragment)?);
    let name = |s: &str| CString::new(s).unwrap_or_default();
    // SAFETY: a current context and two compiled shaders.
    unsafe {
        let id = (gl.glCreateProgram)();
        (gl.glAttachShader)(id, vs);
        (gl.glAttachShader)(id, fs);
        (gl.glBindAttribLocation)(id, 0, name("a_unit").as_ptr());
        (gl.glLinkProgram)(id);
        let mut ok = 0;
        (gl.glGetProgramiv)(id, GL_LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut log = vec![0u8; 1024];
            let mut n = 0;
            (gl.glGetProgramInfoLog)(id, log.len() as i32, &mut n, log.as_mut_ptr().cast());
            return Err(format!("GY-VIUDA-GPU: link: {}", String::from_utf8_lossy(&log[..n.max(0) as usize])));
        }
        let at = |u: &str| (gl.glGetUniformLocation)(id, name(u).as_ptr());
        Ok(Program { id, box_: at("u_box"), size: at("u_size"), a: at(uniforms[0]), b: at(uniforms[1]), c: at(uniforms[2]) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Viuda;

    /// Pixels whose largest channel difference exceeds `tolerance`, and the
    /// largest difference.
    fn compare(a: &Pixmap, b: &Pixmap, tolerance: u8) -> (usize, u8) {
        let mut over = 0;
        let mut worst = 0;
        for (p, q) in a.data().chunks(4).zip(b.data().chunks(4)) {
            let d = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            worst = worst.max(d);
            over += usize::from(d > tolerance);
        }
        (over, worst)
    }

    /// The GPU frame matches the software composition of the same display
    /// list through a game. Skipped where EGL cannot open offscreen, unless
    /// `VIUDA_REQUIRE_GPU` is set.
    #[test]
    fn gpu_frames_match_the_software_composition() {
        let (w, h) = (960, 540);
        let mut gpu = match Gpu::headless(w, h) {
            Ok(g) => g,
            Err(e) => {
                assert!(std::env::var_os("VIUDA_REQUIRE_GPU").is_none(), "{e}");
                eprintln!("skipped: {e}");
                return;
            }
        };
        eprintln!("{}", gpu.info);
        let Ok((font, _)) = crate::system_font() else { return };
        let source = include_str!("../../../apps/pairs.gscene");
        let mut v = Viuda::load(source, font, w, h).expect("pairs loads");
        let mut frames = 0;
        let mut uploads = 0;
        for key in [None, Some(crate::Key::Right), Some(crate::Key::Ok), Some(crate::Key::Down), Some(crate::Key::Ok)] {
            if let Some(k) = key {
                v.key(k);
            }
            loop {
                let busy = v.tick();
                let ops = v.display_list(true);
                let stats = gpu.draw(v.scene(), &ops);
                uploads += stats.uploads;
                gpu.present().expect("present");
                let got = gpu.read().expect("offscreen");
                let (over, worst) = compare(&got, &v.scene().compose(&ops), 2);
                eprintln!("{key:?}: {over} pixels over 2 (worst {worst}); {stats:?}");
                assert!(over * 10_000 < (w * h) as usize, "{key:?}: {over} pixels over 2, worst {worst}");
                frames += 1;
                if !busy {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        }
        assert!(frames >= 5);
        assert!(uploads > 0 && gpu.resident_bytes() > 0);
    }
}
