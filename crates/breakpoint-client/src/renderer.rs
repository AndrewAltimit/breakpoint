use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4};
use wasm_bindgen::JsCast;
use web_sys::{
    WebGl2RenderingContext as GL, WebGlProgram, WebGlShader, WebGlUniformLocation,
    WebGlVertexArrayObject,
};

use crate::camera_gl::Camera;
use crate::scene::{MaterialType, MeshType, Scene};

/// Shader program with cached uniform locations.
struct ShaderProgram {
    program: WebGlProgram,
    u_mvp: Option<WebGlUniformLocation>,
    u_model: Option<WebGlUniformLocation>,
    u_color: Option<WebGlUniformLocation>,
    u_color_start: Option<WebGlUniformLocation>,
    u_color_end: Option<WebGlUniformLocation>,
    u_time: Option<WebGlUniformLocation>,
    u_ring_count: Option<WebGlUniformLocation>,
    u_speed: Option<WebGlUniformLocation>,
    u_intensity: Option<WebGlUniformLocation>,
    u_camera_pos: Option<WebGlUniformLocation>,
    u_fog_density: Option<WebGlUniformLocation>,
}

/// Cached mesh GPU buffers.
struct MeshBuffers {
    vao: WebGlVertexArrayObject,
    vertex_count: i32,
}

/// WebGL2 renderer.
pub struct Renderer {
    gl: GL,
    canvas_width: u32,
    canvas_height: u32,
    dpr: f64,
    programs: HashMap<&'static str, ShaderProgram>,
    meshes: HashMap<MeshKey, MeshBuffers>,
    time: f32,
    context_lost: std::cell::Cell<bool>,
}

/// Key for mesh cache — identifies unique mesh configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MeshKey {
    Sphere { segments: u16 },
    Cylinder { segments: u16 },
    Cuboid,
    Plane,
}

impl From<&MeshType> for MeshKey {
    fn from(m: &MeshType) -> Self {
        match *m {
            MeshType::Sphere { segments } => MeshKey::Sphere { segments },
            MeshType::Cylinder { segments } => MeshKey::Cylinder { segments },
            MeshType::Cuboid => MeshKey::Cuboid,
            MeshType::Plane => MeshKey::Plane,
        }
    }
}

impl Renderer {
    /// Initialize the renderer from the `#game-canvas` element.
    pub fn new() -> Result<Self, String> {
        let window = web_sys::window().ok_or("No window")?;
        let document = window.document().ok_or("No document")?;
        let canvas = document
            .get_element_by_id("game-canvas")
            .ok_or("No #game-canvas")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| "Not a canvas element")?;

        // Explicit context attributes — required for Firefox compatibility.
        // Chrome auto-defaults these, but Firefox may crash or fail without them.
        let attrs = web_sys::WebGlContextAttributes::new();
        attrs.set_antialias(true);
        attrs.set_depth(true);
        attrs.set_stencil(false);
        attrs.set_alpha(true);
        attrs.set_premultiplied_alpha(true);
        attrs.set_preserve_drawing_buffer(false);

        let gl = canvas
            .get_context_with_context_options("webgl2", &attrs)
            .map_err(|e| format!("getContext failed: {e:?}"))?
            .ok_or("WebGL2 not supported")?
            .dyn_into::<GL>()
            .map_err(|_| "Not a WebGl2RenderingContext")?;

        let dpr = window.device_pixel_ratio();

        gl.enable(GL::DEPTH_TEST);
        gl.enable(GL::BLEND);
        gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
        gl.enable(GL::CULL_FACE);

        let context_lost = std::cell::Cell::new(false);

        // Listen for WebGL context loss/restore events
        {
            let canvas_el: web_sys::EventTarget = canvas.clone().into();

            let lost_flag = context_lost.clone();
            let on_lost = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
                move |evt: web_sys::Event| {
                    evt.prevent_default(); // allow context restore
                    lost_flag.set(true);
                    web_sys::console::warn_1(&"WebGL context lost".into());
                },
            );
            let _ = canvas_el.add_event_listener_with_callback(
                "webglcontextlost",
                on_lost.as_ref().unchecked_ref(),
            );
            on_lost.forget(); // lives as long as the canvas

            let restore_flag = context_lost.clone();
            let gl_restore = gl.clone();
            let on_restore = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
                move |_evt: web_sys::Event| {
                    restore_flag.set(false);
                    web_sys::console::log_1(&"WebGL context restored".into());
                    // Re-enable GL state (programs/meshes rebuilt on next draw)
                    gl_restore.enable(GL::DEPTH_TEST);
                    gl_restore.enable(GL::BLEND);
                    gl_restore.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
                    gl_restore.enable(GL::CULL_FACE);
                },
            );
            let _ = canvas_el.add_event_listener_with_callback(
                "webglcontextrestored",
                on_restore.as_ref().unchecked_ref(),
            );
            on_restore.forget(); // lives as long as the canvas
        }

        let mut renderer = Self {
            gl,
            canvas_width: 0,
            canvas_height: 0,
            dpr,
            programs: HashMap::new(),
            meshes: HashMap::new(),
            time: 0.0,
            context_lost,
        };

        renderer.compile_programs()?;
        renderer.generate_meshes();
        renderer.resize();

        Ok(renderer)
    }

    /// Returns true if the WebGL context is currently lost.
    pub fn is_context_lost(&self) -> bool {
        self.context_lost.get()
    }

    /// Rebuild GPU resources after context restore.
    pub fn rebuild_resources(&mut self) -> Result<(), String> {
        self.programs.clear();
        self.meshes.clear();
        self.compile_programs()?;
        self.generate_meshes();
        Ok(())
    }

    /// Get the current device pixel ratio.
    pub fn dpr(&self) -> f64 {
        self.dpr
    }

    /// Canvas size in CSS pixels.
    pub fn viewport_size(&self) -> (f32, f32) {
        let css_w = self.canvas_width as f64 / self.dpr;
        let css_h = self.canvas_height as f64 / self.dpr;
        (css_w as f32, css_h as f32)
    }

    /// Check and apply canvas resize if needed. Returns true if resized.
    pub fn resize(&mut self) -> bool {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return false,
        };
        self.dpr = window.device_pixel_ratio();

        let canvas = match self.gl.canvas() {
            Some(c) => c.dyn_into::<web_sys::HtmlCanvasElement>().ok(),
            None => None,
        };
        let Some(canvas) = canvas else {
            return false;
        };

        let display_w = (canvas.client_width() as f64 * self.dpr) as u32;
        let display_h = (canvas.client_height() as f64 * self.dpr) as u32;

        if display_w == 0 || display_h == 0 {
            return false;
        }

        if canvas.width() != display_w || canvas.height() != display_h {
            canvas.set_width(display_w);
            canvas.set_height(display_h);
            self.gl.viewport(0, 0, display_w as i32, display_h as i32);
            self.canvas_width = display_w;
            self.canvas_height = display_h;
            true
        } else {
            self.canvas_width = display_w;
            self.canvas_height = display_h;
            false
        }
    }

    /// Project a world-space position to screen-space (CSS pixels).
    /// Returns `None` if the point is behind the camera.
    pub fn world_to_screen(&self, world_pos: Vec3, vp: &Mat4) -> Option<(f32, f32)> {
        let clip = *vp * world_pos.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let (vw, vh) = self.viewport_size();
        let sx = (ndc_x * 0.5 + 0.5) * vw;
        let sy = (1.0 - (ndc_y * 0.5 + 0.5)) * vh;
        Some((sx, sy))
    }

    /// Render the scene with the given camera.
    pub fn draw(
        &mut self,
        scene: &Scene,
        camera: &Camera,
        dt: f32,
        clear_color: Vec4,
        fog_density: f32,
    ) {
        self.time += dt;

        // Skip rendering while context is lost; rebuild on restore
        if self.context_lost.get() {
            return;
        }
        if self.programs.is_empty() {
            // Context was just restored — rebuild GPU resources
            if self.rebuild_resources().is_err() {
                return;
            }
        }

        self.resize();

        let gl = &self.gl;
        gl.clear_color(clear_color.x, clear_color.y, clear_color.z, clear_color.w);
        gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);

        let vp = camera.view_projection();

        for obj in scene.visible_objects() {
            let model = obj.transform.matrix();
            let mvp = vp * model;

            let program_name = match &obj.material {
                MaterialType::Unlit { .. } => "unlit",
                MaterialType::Gradient { .. } => "gradient",
                MaterialType::Ripple { .. } => "ripple",
                MaterialType::Glow { .. } => "glow",
                MaterialType::TronWall { .. } => "tronwall",
            };

            let Some(prog) = self.programs.get(program_name) else {
                continue;
            };
            gl.use_program(Some(&prog.program));

            // Common uniforms
            set_mat4(gl, &prog.u_mvp, &mvp);
            set_mat4(gl, &prog.u_model, &model);
            set_vec3(gl, &prog.u_camera_pos, &camera.position);
            set_f32(gl, &prog.u_fog_density, fog_density);

            // Material-specific uniforms
            match &obj.material {
                MaterialType::Unlit { color } => {
                    set_vec4(gl, &prog.u_color, color);
                },
                MaterialType::Gradient { start, end } => {
                    set_vec4(gl, &prog.u_color_start, start);
                    set_vec4(gl, &prog.u_color_end, end);
                },
                MaterialType::Ripple {
                    color,
                    ring_count,
                    speed,
                } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_time, self.time);
                    set_f32(gl, &prog.u_ring_count, *ring_count);
                    set_f32(gl, &prog.u_speed, *speed);
                },
                MaterialType::Glow { color, intensity } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_intensity, *intensity);
                },
                MaterialType::TronWall { color, intensity } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_intensity, *intensity);
                },
            }

            // Bind mesh and draw
            let mesh_key = MeshKey::from(&obj.mesh);
            if let Some(mesh) = self.meshes.get(&mesh_key) {
                gl.bind_vertex_array(Some(&mesh.vao));
                gl.draw_arrays(GL::TRIANGLES, 0, mesh.vertex_count);
            }
        }

        self.gl.bind_vertex_array(None);
    }

    /// Compile all shader programs.
    fn compile_programs(&mut self) -> Result<(), String> {
        let vert_src = include_str!("shaders_gl/unlit.vert");

        let configs: Vec<(&str, &str)> = vec![
            ("unlit", include_str!("shaders_gl/unlit.frag")),
            ("gradient", include_str!("shaders_gl/gradient.frag")),
            ("ripple", include_str!("shaders_gl/ripple.frag")),
            ("glow", include_str!("shaders_gl/glow.frag")),
            ("tronwall", include_str!("shaders_gl/tronwall.frag")),
        ];

        for (name, frag_src) in configs {
            let program = link_program(&self.gl, vert_src, frag_src)?;

            let sp = ShaderProgram {
                u_mvp: self.gl.get_uniform_location(&program, "u_mvp"),
                u_model: self.gl.get_uniform_location(&program, "u_model"),
                u_color: self.gl.get_uniform_location(&program, "u_color"),
                u_color_start: self.gl.get_uniform_location(&program, "u_color_start"),
                u_color_end: self.gl.get_uniform_location(&program, "u_color_end"),
                u_time: self.gl.get_uniform_location(&program, "u_time"),
                u_ring_count: self.gl.get_uniform_location(&program, "u_ring_count"),
                u_speed: self.gl.get_uniform_location(&program, "u_speed"),
                u_intensity: self.gl.get_uniform_location(&program, "u_intensity"),
                u_camera_pos: self.gl.get_uniform_location(&program, "u_camera_pos"),
                u_fog_density: self.gl.get_uniform_location(&program, "u_fog_density"),
                program,
            };
            self.programs.insert(name, sp);
        }
        Ok(())
    }

    /// Generate mesh VBOs/VAOs for each primitive type.
    fn generate_meshes(&mut self) {
        let configs: Vec<(MeshKey, Vec<f32>)> = vec![
            (MeshKey::Cuboid, generate_cuboid()),
            (MeshKey::Plane, generate_plane()),
            (MeshKey::Sphere { segments: 16 }, generate_sphere(16)),
            (MeshKey::Cylinder { segments: 16 }, generate_cylinder(16)),
        ];

        for (key, vertices) in configs {
            if let Some(buffers) = self.upload_mesh(&vertices) {
                self.meshes.insert(key, buffers);
            }
        }
    }

    /// Upload interleaved vertex data (pos3 + normal3 + uv2 = 8 floats per vertex).
    fn upload_mesh(&self, data: &[f32]) -> Option<MeshBuffers> {
        let gl = &self.gl;

        let vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(&vao));

        let buffer = gl.create_buffer()?;
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&buffer));

        unsafe {
            let view = js_sys::Float32Array::view(data);
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, GL::STATIC_DRAW);
        }

        let stride = 8 * 4; // 8 floats * 4 bytes
        // position (location 0)
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 3, GL::FLOAT, false, stride, 0);
        // normal (location 1)
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(1, 3, GL::FLOAT, false, stride, 12);
        // uv (location 2)
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_with_i32(2, 2, GL::FLOAT, false, stride, 24);

        gl.bind_vertex_array(None);

        let vertex_count = data.len() as i32 / 8;
        Some(MeshBuffers { vao, vertex_count })
    }
}

// --- Uniform helpers ---

fn set_mat4(gl: &GL, loc: &Option<WebGlUniformLocation>, m: &Mat4) {
    if let Some(loc) = loc {
        gl.uniform_matrix4fv_with_f32_array(Some(loc), false, m.as_ref());
    }
}

fn set_vec3(gl: &GL, loc: &Option<WebGlUniformLocation>, v: &Vec3) {
    if let Some(loc) = loc {
        gl.uniform3f(Some(loc), v.x, v.y, v.z);
    }
}

fn set_vec4(gl: &GL, loc: &Option<WebGlUniformLocation>, v: &Vec4) {
    if let Some(loc) = loc {
        gl.uniform4f(Some(loc), v.x, v.y, v.z, v.w);
    }
}

fn set_f32(gl: &GL, loc: &Option<WebGlUniformLocation>, v: f32) {
    if let Some(loc) = loc {
        gl.uniform1f(Some(loc), v);
    }
}

// --- Shader compilation ---

fn compile_shader(gl: &GL, shader_type: u32, source: &str) -> Result<WebGlShader, String> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or("Failed to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    if gl
        .get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        let log = gl.get_shader_info_log(&shader).unwrap_or_default();
        web_sys::console::error_1(&format!("Shader compile error: {log}").into());
        gl.delete_shader(Some(&shader));
        Err(format!("Shader compile error: {log}"))
    }
}

fn link_program(gl: &GL, vert_src: &str, frag_src: &str) -> Result<WebGlProgram, String> {
    let vert = compile_shader(gl, GL::VERTEX_SHADER, vert_src)?;
    let frag = compile_shader(gl, GL::FRAGMENT_SHADER, frag_src)?;

    let program = gl.create_program().ok_or("Failed to create program")?;
    gl.attach_shader(&program, &vert);
    gl.attach_shader(&program, &frag);
    gl.link_program(&program);

    // Shaders can be deleted after linking
    gl.delete_shader(Some(&vert));
    gl.delete_shader(Some(&frag));

    if gl
        .get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        let log = gl.get_program_info_log(&program).unwrap_or_default();
        web_sys::console::error_1(&format!("Program link error: {log}").into());
        gl.delete_program(Some(&program));
        Err(format!("Program link error: {log}"))
    }
}

// --- Mesh generation (interleaved: pos3 + normal3 + uv2) ---

fn push_vertex(buf: &mut Vec<f32>, pos: Vec3, normal: Vec3, u: f32, v: f32) {
    buf.extend_from_slice(&[pos.x, pos.y, pos.z, normal.x, normal.y, normal.z, u, v]);
}

fn generate_cuboid() -> Vec<f32> {
    let mut buf = Vec::with_capacity(36 * 8);
    // Unit cuboid centered at origin, half-extents = 0.5
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        // front (+Z)
        (Vec3::Z, Vec3::X, Vec3::Y),
        // back (-Z)
        (-Vec3::Z, -Vec3::X, Vec3::Y),
        // right (+X)
        (Vec3::X, -Vec3::Z, Vec3::Y),
        // left (-X)
        (-Vec3::X, Vec3::Z, Vec3::Y),
        // top (+Y)
        (Vec3::Y, Vec3::X, -Vec3::Z),
        // bottom (-Y)
        (-Vec3::Y, Vec3::X, Vec3::Z),
    ];

    for (normal, right, up) in faces {
        let center = normal * 0.5;
        let r = right * 0.5;
        let u = up * 0.5;
        let v00 = center - r - u;
        let v10 = center + r - u;
        let v11 = center + r + u;
        let v01 = center - r + u;
        // Two triangles
        push_vertex(&mut buf, v00, normal, 0.0, 0.0);
        push_vertex(&mut buf, v10, normal, 1.0, 0.0);
        push_vertex(&mut buf, v11, normal, 1.0, 1.0);
        push_vertex(&mut buf, v00, normal, 0.0, 0.0);
        push_vertex(&mut buf, v11, normal, 1.0, 1.0);
        push_vertex(&mut buf, v01, normal, 0.0, 1.0);
    }
    buf
}

fn generate_plane() -> Vec<f32> {
    let mut buf = Vec::with_capacity(6 * 8);
    let normal = Vec3::Y;
    let h = 0.5;
    // Plane on XZ at Y=0
    let v00 = Vec3::new(-h, 0.0, -h);
    let v10 = Vec3::new(h, 0.0, -h);
    let v11 = Vec3::new(h, 0.0, h);
    let v01 = Vec3::new(-h, 0.0, h);
    push_vertex(&mut buf, v00, normal, 0.0, 0.0);
    push_vertex(&mut buf, v10, normal, 1.0, 0.0);
    push_vertex(&mut buf, v11, normal, 1.0, 1.0);
    push_vertex(&mut buf, v00, normal, 0.0, 0.0);
    push_vertex(&mut buf, v11, normal, 1.0, 1.0);
    push_vertex(&mut buf, v01, normal, 0.0, 1.0);
    buf
}

fn generate_sphere(segments: u16) -> Vec<f32> {
    let mut buf = Vec::new();
    let rings = segments;
    let sectors = segments;

    for r in 0..rings {
        for s in 0..sectors {
            let r0 = r as f32 / rings as f32;
            let r1 = (r + 1) as f32 / rings as f32;
            let s0 = s as f32 / sectors as f32;
            let s1 = (s + 1) as f32 / sectors as f32;

            let p00 = sphere_point(r0, s0);
            let p10 = sphere_point(r1, s0);
            let p11 = sphere_point(r1, s1);
            let p01 = sphere_point(r0, s1);

            // Two triangles per quad
            push_vertex(&mut buf, p00, p00, s0, r0);
            push_vertex(&mut buf, p10, p10, s0, r1);
            push_vertex(&mut buf, p11, p11, s1, r1);
            push_vertex(&mut buf, p00, p00, s0, r0);
            push_vertex(&mut buf, p11, p11, s1, r1);
            push_vertex(&mut buf, p01, p01, s1, r0);
        }
    }
    buf
}

fn sphere_point(ring_frac: f32, sector_frac: f32) -> Vec3 {
    let theta = ring_frac * std::f32::consts::PI;
    let phi = sector_frac * std::f32::consts::TAU;
    Vec3::new(
        theta.sin() * phi.cos() * 0.5,
        theta.cos() * 0.5,
        theta.sin() * phi.sin() * 0.5,
    )
}

fn generate_cylinder(segments: u16) -> Vec<f32> {
    let mut buf = Vec::new();
    let h = 0.5;

    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let u0 = i as f32 / segments as f32;
        let u1 = (i + 1) as f32 / segments as f32;

        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();

        let r = 0.5;
        let p0_bot = Vec3::new(c0 * r, -h, s0 * r);
        let p1_bot = Vec3::new(c1 * r, -h, s1 * r);
        let p0_top = Vec3::new(c0 * r, h, s0 * r);
        let p1_top = Vec3::new(c1 * r, h, s1 * r);

        let n0 = Vec3::new(c0, 0.0, s0);
        let n1 = Vec3::new(c1, 0.0, s1);

        // Side quad
        push_vertex(&mut buf, p0_bot, n0, u0, 0.0);
        push_vertex(&mut buf, p1_bot, n1, u1, 0.0);
        push_vertex(&mut buf, p1_top, n1, u1, 1.0);
        push_vertex(&mut buf, p0_bot, n0, u0, 0.0);
        push_vertex(&mut buf, p1_top, n1, u1, 1.0);
        push_vertex(&mut buf, p0_top, n0, u0, 1.0);

        // Top cap
        push_vertex(&mut buf, Vec3::new(0.0, h, 0.0), Vec3::Y, 0.5, 0.5);
        push_vertex(&mut buf, p0_top, Vec3::Y, u0, 1.0);
        push_vertex(&mut buf, p1_top, Vec3::Y, u1, 1.0);

        // Bottom cap
        push_vertex(&mut buf, Vec3::new(0.0, -h, 0.0), -Vec3::Y, 0.5, 0.5);
        push_vertex(&mut buf, p1_bot, -Vec3::Y, u1, 0.0);
        push_vertex(&mut buf, p0_bot, -Vec3::Y, u0, 0.0);
    }
    buf
}
