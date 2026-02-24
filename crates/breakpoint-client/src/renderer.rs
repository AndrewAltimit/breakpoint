use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4};
use wasm_bindgen::JsCast;
use web_sys::{
    WebGl2RenderingContext as GL, WebGlFramebuffer, WebGlProgram, WebGlRenderbuffer, WebGlShader,
    WebGlTexture, WebGlUniformLocation, WebGlVertexArrayObject,
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
    u_fog_color: Option<WebGlUniformLocation>,
    u_resolution: Option<WebGlUniformLocation>,
    // Sprite shader uniforms
    u_sprite_rect: Option<WebGlUniformLocation>,
    u_tint: Option<WebGlUniformLocation>,
    u_flip_x: Option<WebGlUniformLocation>,
    u_texture: Option<WebGlUniformLocation>,
    u_outline_width: Option<WebGlUniformLocation>,
    u_dissolve: Option<WebGlUniformLocation>,
    /// Palette texture unit (deferred: indexed palette rendering not yet active).
    #[allow(dead_code)]
    u_palette: Option<WebGlUniformLocation>,
    u_use_palette: Option<WebGlUniformLocation>,
    // Parallax shader uniforms
    u_uv_offset: Option<WebGlUniformLocation>,
    u_uv_scale: Option<WebGlUniformLocation>,
    // Water shader uniforms
    u_depth: Option<WebGlUniformLocation>,
    u_wave_speed: Option<WebGlUniformLocation>,
    // Lighting uniforms (for lit sprite shader, 32 colored lights)
    u_lights: Vec<Option<WebGlUniformLocation>>,
    u_light_color: Vec<Option<WebGlUniformLocation>>,
    u_light_count: Option<WebGlUniformLocation>,
    u_ambient: Option<WebGlUniformLocation>,
    u_ambient_color: Option<WebGlUniformLocation>,
    // GBA-style color ramp uniforms
    u_ramp_shadow: Option<WebGlUniformLocation>,
    u_ramp_mid: Option<WebGlUniformLocation>,
    u_ramp_highlight: Option<WebGlUniformLocation>,
    u_posterize: Option<WebGlUniformLocation>,
    // Whip trail uniforms
    u_arc_progress: Option<WebGlUniformLocation>,
    // Post-process uniforms
    u_scene_texture: Option<WebGlUniformLocation>,
    u_scanline_intensity: Option<WebGlUniformLocation>,
    u_bloom_intensity: Option<WebGlUniformLocation>,
    u_vignette_intensity: Option<WebGlUniformLocation>,
    u_crt_curvature: Option<WebGlUniformLocation>,
    u_grade_shadows: Option<WebGlUniformLocation>,
    u_grade_highlights: Option<WebGlUniformLocation>,
    u_grade_contrast: Option<WebGlUniformLocation>,
    u_saturation: Option<WebGlUniformLocation>,
    u_chromatic_aberration: Option<WebGlUniformLocation>,
    u_film_grain: Option<WebGlUniformLocation>,
}

/// Cached mesh GPU buffers.
struct MeshBuffers {
    vao: WebGlVertexArrayObject,
    vertex_count: i32,
}

/// Post-processing framebuffer resources.
struct PostProcessFBO {
    framebuffer: WebGlFramebuffer,
    color_texture: WebGlTexture,
    depth_renderbuffer: WebGlRenderbuffer,
    width: u32,
    height: u32,
}

/// Post-processing configuration.
pub struct PostProcessConfig {
    pub scanline_intensity: f32,
    pub bloom_intensity: f32,
    pub vignette_intensity: f32,
    pub crt_curvature: f32,
    /// Per-room color grading: shadow tint (RGB).
    pub grade_shadows: [f32; 3],
    /// Per-room color grading: highlight tint (RGB).
    pub grade_highlights: [f32; 3],
    /// Contrast adjustment (1.0 = neutral).
    pub grade_contrast: f32,
    /// Color saturation (1.0 = neutral, 0.0 = grayscale).
    pub saturation: f32,
    /// Chromatic aberration strength in pixels (0.0 = off, triggered on damage).
    pub chromatic_aberration: f32,
    /// Film grain intensity (0.0 = off).
    pub film_grain: f32,
}

impl Default for PostProcessConfig {
    fn default() -> Self {
        Self {
            scanline_intensity: 0.0,
            bloom_intensity: 0.0,
            vignette_intensity: 0.0,
            crt_curvature: 0.0,
            grade_shadows: [1.0, 1.0, 1.0],
            grade_highlights: [1.0, 1.0, 1.0],
            grade_contrast: 1.0,
            saturation: 1.0,
            chromatic_aberration: 0.0,
            film_grain: 0.0,
        }
    }
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
    /// Texture atlases keyed by ID.
    atlases: HashMap<u8, WebGlTexture>,
    /// Palette textures keyed by ID (256x1 RGBA, for indexed color mode).
    /// Deferred: not yet populated; infra for future indexed palette rendering.
    #[allow(dead_code)]
    palettes: HashMap<u8, WebGlTexture>,
    /// Post-processing FBO (created lazily on first draw with post-fx).
    post_fbo: Option<PostProcessFBO>,
    /// Post-processing settings.
    pub post_process: PostProcessConfig,
    /// Sprite batch: reusable CPU-side vertex buffer (cleared each frame).
    batch_vertices: Vec<f32>,
    /// Sprite batch: VAO + VBO for dynamic upload (created lazily).
    batch_vao: Option<WebGlVertexArrayObject>,
    batch_vbo: Option<web_sys::WebGlBuffer>,
}

/// Key for mesh cache — identifies unique mesh configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MeshKey {
    Sphere { segments: u16 },
    Cylinder { segments: u16 },
    Cuboid,
    Plane,
    Quad,
}

impl From<&MeshType> for MeshKey {
    fn from(m: &MeshType) -> Self {
        match *m {
            MeshType::Sphere { segments } => MeshKey::Sphere { segments },
            MeshType::Cylinder { segments } => MeshKey::Cylinder { segments },
            MeshType::Cuboid => MeshKey::Cuboid,
            MeshType::Plane => MeshKey::Plane,
            MeshType::Quad => MeshKey::Quad,
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
            atlases: HashMap::new(),
            palettes: HashMap::new(),
            post_fbo: None,
            post_process: PostProcessConfig::default(),
            batch_vertices: Vec::with_capacity(9 * 6 * 1024),
            batch_vao: None,
            batch_vbo: None,
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
        self.atlases.clear();
        self.palettes.clear();
        // Post-process FBO is GPU-side only; invalidated by context loss.
        self.post_fbo = None;
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

        // If any post-processing effects are enabled, render to FBO
        let use_postfx = self.post_process.scanline_intensity > 0.01
            || self.post_process.bloom_intensity > 0.01
            || self.post_process.vignette_intensity > 0.01
            || self.post_process.crt_curvature > 0.01
            || self.post_process.chromatic_aberration > 0.01
            || self.post_process.film_grain > 0.01
            || (self.post_process.grade_contrast - 1.0).abs() > 0.01
            || (self.post_process.saturation - 1.0).abs() > 0.01;

        if use_postfx {
            self.ensure_post_fbo();
            if let Some(fbo) = &self.post_fbo {
                self.gl
                    .bind_framebuffer(GL::FRAMEBUFFER, Some(&fbo.framebuffer));
                self.gl.viewport(0, 0, fbo.width as i32, fbo.height as i32);
            }
        }

        self.gl
            .clear_color(clear_color.x, clear_color.y, clear_color.z, clear_color.w);
        self.gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);

        let vp = camera.view_projection();

        // Frustum culling: extract frustum planes from the VP matrix and skip
        // objects whose bounding sphere is entirely outside any plane. This avoids
        // draw calls for off-screen objects (significant for Tron with 500+ walls).
        let frustum = extract_frustum_planes(&vp);

        // Sort by shader program to minimize expensive gl.use_program() switches.
        // With ~500 wall segments interleaved with other materials, this reduces
        // program switches from hundreds to ~5 (one per unique program).
        let mut sorted: Vec<&crate::scene::RenderObject> = scene
            .visible_objects()
            .filter(|obj| {
                let pos = obj.transform.translation;
                let max_scale = obj.transform.scale.max_element();
                // Conservative bounding radius: half-diagonal of scaled unit primitive
                let radius = max_scale * 0.87; // sqrt(3)/2 ≈ 0.87
                is_sphere_in_frustum(&frustum, pos, radius)
            })
            .collect();
        sorted.sort_by_key(|obj| material_sort_key(&obj.material));

        // --- Sprite batching: draw simple sprites (no outline, no dissolve) in bulk ---
        // Partition sprites by blend mode for batched drawing.
        let has_batch_program = self.programs.contains_key("sprite_batch");
        if has_batch_program {
            self.ensure_batch_vao();
            self.draw_sprite_batches(&sorted, &vp, scene, fog_density, camera);
        }

        let gl = &self.gl;
        let mut active_program: &str = "";
        // Track whether lighting uniforms have been set for the current sprite program.
        // These are scene-global (same for all sprites), so we set them once per program switch.
        let mut sprite_lighting_set = false;
        for obj in &sorted {
            // Skip sprites that were drawn in the batch pass
            if has_batch_program
                && matches!(
                    &obj.material,
                    MaterialType::Sprite { outline, dissolve, .. }
                        if *outline == 0.0 && *dissolve == 0.0
                )
            {
                continue;
            }
            let model = obj.transform.matrix();
            let mvp = vp * model;

            let program_name = match &obj.material {
                MaterialType::Unlit { .. } => "unlit",
                MaterialType::Gradient { .. } => "gradient",
                MaterialType::Ripple { .. } => "ripple",
                MaterialType::Glow { .. } => "glow",
                MaterialType::TronWall { .. } => "tronwall",
                MaterialType::Sprite { .. } => "sprite",
                MaterialType::Parallax { .. } => "parallax",
                MaterialType::Water { .. } => "water",
                MaterialType::WhipTrail { .. } => "whip",
                MaterialType::SlashArc { .. } => "slash_arc",
                MaterialType::MagicCircle { .. } => "magic_circle",
                MaterialType::GodRays { .. } => "godrays",
                MaterialType::FogLayer { .. } => "fog_layer",
                MaterialType::HealthBar { .. } => "health_bar",
            };

            let Some(prog) = self.programs.get(program_name) else {
                continue;
            };
            // Only switch program when the material type changes
            if program_name != active_program {
                gl.use_program(Some(&prog.program));
                active_program = program_name;
                sprite_lighting_set = false;
            }

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
                    set_f32(gl, &prog.u_time, self.time);
                    // Derive tile count from geometry so noise cells stay square
                    let s = obj.transform.scale;
                    let width = s.x.max(s.z);
                    let tiles = width / s.y.max(0.01);
                    set_vec2(gl, &prog.u_resolution, tiles, 3.0);
                },
                MaterialType::Sprite {
                    atlas_id,
                    sprite_rect,
                    tint,
                    flip_x,
                    dissolve,
                    outline,
                    blend_mode,
                } => {
                    // Bind atlas texture
                    gl.active_texture(GL::TEXTURE0);
                    if let Some(tex) = self.atlases.get(atlas_id) {
                        gl.bind_texture(GL::TEXTURE_2D, Some(tex));
                    }
                    if let Some(loc) = &prog.u_texture {
                        gl.uniform1i(Some(loc), 0);
                    }
                    set_vec4(gl, &prog.u_sprite_rect, sprite_rect);
                    set_vec4(gl, &prog.u_tint, tint);
                    set_f32(gl, &prog.u_flip_x, if *flip_x { 1.0 } else { 0.0 });
                    set_f32(gl, &prog.u_outline_width, *outline);
                    set_f32(gl, &prog.u_dissolve, *dissolve);
                    set_f32(gl, &prog.u_use_palette, 0.0);
                    // Blend mode
                    match blend_mode {
                        crate::scene::BlendMode::Normal => {
                            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
                            gl.blend_equation(GL::FUNC_ADD);
                        },
                        crate::scene::BlendMode::Additive => {
                            gl.blend_func(GL::SRC_ALPHA, GL::ONE);
                            gl.blend_equation(GL::FUNC_ADD);
                        },
                        crate::scene::BlendMode::Subtractive => {
                            gl.blend_func(GL::SRC_ALPHA, GL::ONE);
                            gl.blend_equation(GL::FUNC_REVERSE_SUBTRACT);
                        },
                    }
                    // Set lighting uniforms ONCE per sprite program switch
                    // (lights/ambient/ramp are scene-global, same for all sprites)
                    if !sprite_lighting_set {
                        sprite_lighting_set = true;
                        let light_count = scene.lighting.lights.len().min(32) as i32;
                        if let Some(loc) = &prog.u_light_count {
                            gl.uniform1i(Some(loc), light_count);
                        }
                        set_f32(gl, &prog.u_ambient, scene.lighting.ambient);
                        if let Some(loc) = &prog.u_ambient_color {
                            let ac = &scene.lighting.ambient_color;
                            gl.uniform3f(Some(loc), ac[0], ac[1], ac[2]);
                        }
                        if let Some(loc) = &prog.u_ramp_shadow {
                            let rs = &scene.lighting.ramp_shadow;
                            gl.uniform3f(Some(loc), rs[0], rs[1], rs[2]);
                        }
                        if let Some(loc) = &prog.u_ramp_mid {
                            let rm = &scene.lighting.ramp_mid;
                            gl.uniform3f(Some(loc), rm[0], rm[1], rm[2]);
                        }
                        if let Some(loc) = &prog.u_ramp_highlight {
                            let rh = &scene.lighting.ramp_highlight;
                            gl.uniform3f(Some(loc), rh[0], rh[1], rh[2]);
                        }
                        set_f32(gl, &prog.u_posterize, scene.lighting.posterize);
                        if let Some(loc) = &prog.u_fog_color {
                            let fc = &scene.lighting.fog_color;
                            gl.uniform3f(Some(loc), fc[0], fc[1], fc[2]);
                        }
                        for (i, light) in scene.lighting.lights.iter().take(32).enumerate() {
                            if let Some(loc) = prog.u_lights.get(i).and_then(|l| l.as_ref()) {
                                gl.uniform4f(Some(loc), light[0], light[1], light[2], light[3]);
                            }
                            if let Some(loc) = prog.u_light_color.get(i).and_then(|l| l.as_ref()) {
                                let c = scene
                                    .lighting
                                    .light_colors
                                    .get(i)
                                    .copied()
                                    .unwrap_or([1.0, 1.0, 1.0, 0.0]);
                                gl.uniform4f(Some(loc), c[0], c[1], c[2], c[3]);
                            }
                        }
                    }
                    // Disable backface culling for sprites
                    gl.disable(GL::CULL_FACE);
                },
                MaterialType::Parallax {
                    atlas_id,
                    layer_rect,
                    scroll_factor,
                    tint,
                } => {
                    gl.active_texture(GL::TEXTURE0);
                    if let Some(tex) = self.atlases.get(atlas_id) {
                        gl.bind_texture(GL::TEXTURE_2D, Some(tex));
                    }
                    if let Some(loc) = &prog.u_texture {
                        gl.uniform1i(Some(loc), 0);
                    }
                    // UV offset: scroll based on camera X position
                    let scroll_x = camera.position.x * scroll_factor * 0.05;
                    set_vec2(gl, &prog.u_uv_offset, scroll_x, layer_rect.y);
                    // UV scale: full width, layer height portion
                    set_vec2(gl, &prog.u_uv_scale, 1.0, layer_rect.w - layer_rect.y);
                    set_vec4(gl, &prog.u_tint, tint);
                    set_f32(gl, &prog.u_time, self.time);
                    set_f32(gl, &prog.u_intensity, 0.0); // sway amplitude
                    set_f32(gl, &prog.u_speed, 1.0); // crossfade alpha (fully visible)
                    // Disable backface culling + depth write for background
                    gl.disable(GL::CULL_FACE);
                    gl.depth_mask(false);
                },
                MaterialType::Water {
                    color,
                    depth,
                    wave_speed,
                } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_depth, *depth);
                    set_f32(gl, &prog.u_wave_speed, *wave_speed);
                    set_f32(gl, &prog.u_time, self.time);
                    // Transparent water: disable culling, keep depth writes
                    gl.disable(GL::CULL_FACE);
                },
                MaterialType::WhipTrail { progress, color } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_arc_progress, *progress);
                    set_f32(gl, &prog.u_time, self.time);
                    // Additive blending for bright whip effect
                    gl.blend_func(GL::SRC_ALPHA, GL::ONE);
                    gl.disable(GL::CULL_FACE);
                    gl.disable(GL::DEPTH_TEST);
                },
                MaterialType::SlashArc {
                    progress,
                    angle,
                    color,
                } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_arc_progress, *progress);
                    set_f32(gl, &prog.u_intensity, *angle); // reuse u_intensity for arc_angle
                    set_f32(gl, &prog.u_time, self.time);
                    gl.blend_func(GL::SRC_ALPHA, GL::ONE);
                    gl.disable(GL::CULL_FACE);
                    gl.disable(GL::DEPTH_TEST);
                },
                MaterialType::MagicCircle {
                    rotation,
                    pulse,
                    color,
                } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_time, self.time);
                    set_f32(gl, &prog.u_speed, *rotation); // reuse u_speed for rotation
                    set_f32(gl, &prog.u_intensity, *pulse); // reuse u_intensity for pulse
                    gl.blend_func(GL::SRC_ALPHA, GL::ONE);
                    gl.disable(GL::CULL_FACE);
                    gl.disable(GL::DEPTH_TEST);
                },
                MaterialType::GodRays { intensity, color } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_intensity, *intensity);
                    set_f32(gl, &prog.u_time, self.time);
                    set_f32(gl, &prog.u_speed, 0.5);
                    gl.blend_func(GL::SRC_ALPHA, GL::ONE);
                    gl.disable(GL::CULL_FACE);
                    gl.disable(GL::DEPTH_TEST);
                },
                MaterialType::FogLayer { density, color } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_intensity, *density);
                    set_f32(gl, &prog.u_time, self.time);
                    gl.disable(GL::CULL_FACE);
                    gl.depth_mask(false);
                },
                MaterialType::HealthBar { fill, color } => {
                    set_vec4(gl, &prog.u_color, color);
                    set_f32(gl, &prog.u_intensity, *fill);
                    set_f32(gl, &prog.u_time, self.time);
                    gl.disable(GL::CULL_FACE);
                    gl.disable(GL::DEPTH_TEST);
                },
            }

            // Bind mesh and draw
            let mesh_key = MeshKey::from(&obj.mesh);
            if let Some(mesh) = self.meshes.get(&mesh_key) {
                gl.bind_vertex_array(Some(&mesh.vao));
                gl.draw_arrays(GL::TRIANGLES, 0, mesh.vertex_count);
            }

            // Restore GL state modified by material-specific setup
            match &obj.material {
                MaterialType::Parallax { .. } | MaterialType::FogLayer { .. } => {
                    gl.depth_mask(true);
                    gl.enable(GL::CULL_FACE);
                },
                MaterialType::Sprite { blend_mode, .. } => {
                    if !matches!(blend_mode, crate::scene::BlendMode::Normal) {
                        gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
                        gl.blend_equation(GL::FUNC_ADD);
                    }
                    gl.enable(GL::CULL_FACE);
                },
                MaterialType::Water { .. } => {
                    gl.enable(GL::CULL_FACE);
                },
                MaterialType::WhipTrail { .. }
                | MaterialType::SlashArc { .. }
                | MaterialType::MagicCircle { .. }
                | MaterialType::GodRays { .. } => {
                    gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
                    gl.enable(GL::CULL_FACE);
                    gl.enable(GL::DEPTH_TEST);
                },
                MaterialType::HealthBar { .. } => {
                    gl.enable(GL::CULL_FACE);
                    gl.enable(GL::DEPTH_TEST);
                },
                _ => {},
            }
        }

        // Re-enable state that may have been disabled by material batches
        gl.enable(GL::CULL_FACE);
        gl.enable(GL::DEPTH_TEST);
        gl.depth_mask(true);
        gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
        self.gl.bind_vertex_array(None);

        // Post-processing pass: read FBO, draw fullscreen quad with effects
        if use_postfx {
            self.draw_postprocess_pass();
        }
    }

    /// Compile all shader programs.
    fn compile_programs(&mut self) -> Result<(), String> {
        let vert_src = include_str!("shaders_gl/unlit.vert");

        let configs: Vec<(&str, &str, &str)> = vec![
            ("unlit", vert_src, include_str!("shaders_gl/unlit.frag")),
            (
                "gradient",
                vert_src,
                include_str!("shaders_gl/gradient.frag"),
            ),
            ("ripple", vert_src, include_str!("shaders_gl/ripple.frag")),
            ("glow", vert_src, include_str!("shaders_gl/glow.frag")),
            (
                "tronwall",
                vert_src,
                include_str!("shaders_gl/tronwall.frag"),
            ),
            (
                "sprite",
                include_str!("shaders_gl/sprite.vert"),
                include_str!("shaders_gl/sprite.frag"),
            ),
            (
                "sprite_batch",
                include_str!("shaders_gl/sprite_batch.vert"),
                include_str!("shaders_gl/sprite_batch.frag"),
            ),
            (
                "parallax",
                include_str!("shaders_gl/parallax.vert"),
                include_str!("shaders_gl/parallax.frag"),
            ),
            (
                "water",
                include_str!("shaders_gl/water.vert"),
                include_str!("shaders_gl/water.frag"),
            ),
            (
                "whip",
                include_str!("shaders_gl/whip.vert"),
                include_str!("shaders_gl/whip.frag"),
            ),
            (
                "postprocess",
                include_str!("shaders_gl/postprocess.vert"),
                include_str!("shaders_gl/postprocess.frag"),
            ),
            (
                "slash_arc",
                include_str!("shaders_gl/slash_arc.vert"),
                include_str!("shaders_gl/slash_arc.frag"),
            ),
            (
                "magic_circle",
                include_str!("shaders_gl/magic_circle.vert"),
                include_str!("shaders_gl/magic_circle.frag"),
            ),
            (
                "godrays",
                include_str!("shaders_gl/godrays.vert"),
                include_str!("shaders_gl/godrays.frag"),
            ),
            (
                "fog_layer",
                include_str!("shaders_gl/fog_layer.vert"),
                include_str!("shaders_gl/fog_layer.frag"),
            ),
            (
                "health_bar",
                include_str!("shaders_gl/health_bar.vert"),
                include_str!("shaders_gl/health_bar.frag"),
            ),
        ];

        for (name, vs, frag_src) in configs {
            let program = link_program(&self.gl, vs, frag_src)?;

            // Cache light uniform locations (u_lights[0] .. u_lights[31])
            let u_lights: Vec<Option<WebGlUniformLocation>> = (0..32)
                .map(|i| {
                    self.gl
                        .get_uniform_location(&program, &format!("u_lights[{i}]"))
                })
                .collect();
            let u_light_color: Vec<Option<WebGlUniformLocation>> = (0..32)
                .map(|i| {
                    self.gl
                        .get_uniform_location(&program, &format!("u_light_color[{i}]"))
                })
                .collect();

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
                u_fog_color: self.gl.get_uniform_location(&program, "u_fog_color"),
                u_resolution: self.gl.get_uniform_location(&program, "u_resolution"),
                u_sprite_rect: self.gl.get_uniform_location(&program, "u_sprite_rect"),
                u_tint: self.gl.get_uniform_location(&program, "u_tint"),
                u_flip_x: self.gl.get_uniform_location(&program, "u_flip_x"),
                u_texture: self.gl.get_uniform_location(&program, "u_texture"),
                u_outline_width: self.gl.get_uniform_location(&program, "u_outline_width"),
                u_dissolve: self.gl.get_uniform_location(&program, "u_dissolve"),
                u_palette: self.gl.get_uniform_location(&program, "u_palette"),
                u_use_palette: self.gl.get_uniform_location(&program, "u_use_palette"),
                u_uv_offset: self.gl.get_uniform_location(&program, "u_uv_offset"),
                u_uv_scale: self.gl.get_uniform_location(&program, "u_uv_scale"),
                u_depth: self.gl.get_uniform_location(&program, "u_depth"),
                u_wave_speed: self.gl.get_uniform_location(&program, "u_wave_speed"),
                u_lights,
                u_light_color,
                u_light_count: self.gl.get_uniform_location(&program, "u_light_count"),
                u_ambient: self.gl.get_uniform_location(&program, "u_ambient"),
                u_ambient_color: self.gl.get_uniform_location(&program, "u_ambient_color"),
                u_ramp_shadow: self.gl.get_uniform_location(&program, "u_ramp_shadow"),
                u_ramp_mid: self.gl.get_uniform_location(&program, "u_ramp_mid"),
                u_ramp_highlight: self.gl.get_uniform_location(&program, "u_ramp_highlight"),
                u_posterize: self.gl.get_uniform_location(&program, "u_posterize"),
                u_arc_progress: self.gl.get_uniform_location(&program, "u_arc_progress"),
                u_scene_texture: self.gl.get_uniform_location(&program, "u_scene"),
                u_scanline_intensity: self
                    .gl
                    .get_uniform_location(&program, "u_scanline_intensity"),
                u_bloom_intensity: self.gl.get_uniform_location(&program, "u_bloom_intensity"),
                u_vignette_intensity: self
                    .gl
                    .get_uniform_location(&program, "u_vignette_intensity"),
                u_crt_curvature: self.gl.get_uniform_location(&program, "u_crt_curvature"),
                u_grade_shadows: self.gl.get_uniform_location(&program, "u_grade_shadows"),
                u_grade_highlights: self.gl.get_uniform_location(&program, "u_grade_highlights"),
                u_grade_contrast: self.gl.get_uniform_location(&program, "u_grade_contrast"),
                u_saturation: self.gl.get_uniform_location(&program, "u_saturation"),
                u_chromatic_aberration: self
                    .gl
                    .get_uniform_location(&program, "u_chromatic_aberration"),
                u_film_grain: self.gl.get_uniform_location(&program, "u_film_grain"),
                program,
            };
            self.programs.insert(name, sp);
        }
        Ok(())
    }

    /// Load a texture atlas from an HtmlImageElement with NEAREST filtering.
    #[cfg(target_family = "wasm")]
    pub fn load_texture(&mut self, id: u8, img: &web_sys::HtmlImageElement) {
        self.load_texture_with_wrap(id, img, false);
    }

    /// Load a texture with NEAREST filtering and configurable wrapping.
    /// When `wrap_repeat` is true, uses GL::REPEAT for seamless tiling;
    /// otherwise uses CLAMP_TO_EDGE.
    #[cfg(target_family = "wasm")]
    pub fn load_texture_with_wrap(
        &mut self,
        id: u8,
        img: &web_sys::HtmlImageElement,
        wrap_repeat: bool,
    ) {
        let gl = &self.gl;
        let Some(texture) = gl.create_texture() else {
            return;
        };
        gl.bind_texture(GL::TEXTURE_2D, Some(&texture));
        let _ = gl.tex_image_2d_with_u32_and_u32_and_html_image_element(
            GL::TEXTURE_2D,
            0,
            GL::RGBA as i32,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            img,
        );
        // Pixel-art filtering: NEAREST (no blurring)
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::NEAREST as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::NEAREST as i32);
        let wrap = if wrap_repeat {
            GL::REPEAT as i32
        } else {
            GL::CLAMP_TO_EDGE as i32
        };
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, wrap);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, wrap);
        gl.bind_texture(GL::TEXTURE_2D, None);
        self.atlases.insert(id, texture);
    }

    /// Get the accumulated renderer time (for animations).
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Ensure the post-processing FBO exists and matches the canvas size.
    fn ensure_post_fbo(&mut self) {
        let w = self.canvas_width;
        let h = self.canvas_height;

        // If FBO exists and matches size, nothing to do
        if let Some(fbo) = &self.post_fbo {
            if fbo.width == w && fbo.height == h {
                return;
            }
            // Size changed — delete old resources
            self.gl.delete_framebuffer(Some(&fbo.framebuffer));
            self.gl.delete_texture(Some(&fbo.color_texture));
            self.gl.delete_renderbuffer(Some(&fbo.depth_renderbuffer));
            self.post_fbo = None;
        }

        let gl = &self.gl;
        let Some(fb) = gl.create_framebuffer() else {
            return;
        };
        let Some(tex) = gl.create_texture() else {
            return;
        };
        let Some(rb) = gl.create_renderbuffer() else {
            return;
        };

        // Color texture
        gl.bind_texture(GL::TEXTURE_2D, Some(&tex));
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            GL::TEXTURE_2D,
            0,
            GL::RGBA as i32,
            w as i32,
            h as i32,
            0,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            None,
        )
        .ok();
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);

        // Depth renderbuffer
        gl.bind_renderbuffer(GL::RENDERBUFFER, Some(&rb));
        gl.renderbuffer_storage(GL::RENDERBUFFER, GL::DEPTH_COMPONENT16, w as i32, h as i32);

        // Assemble FBO
        gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&fb));
        gl.framebuffer_texture_2d(
            GL::FRAMEBUFFER,
            GL::COLOR_ATTACHMENT0,
            GL::TEXTURE_2D,
            Some(&tex),
            0,
        );
        gl.framebuffer_renderbuffer(
            GL::FRAMEBUFFER,
            GL::DEPTH_ATTACHMENT,
            GL::RENDERBUFFER,
            Some(&rb),
        );

        // Unbind
        gl.bind_framebuffer(GL::FRAMEBUFFER, None);
        gl.bind_texture(GL::TEXTURE_2D, None);
        gl.bind_renderbuffer(GL::RENDERBUFFER, None);

        self.post_fbo = Some(PostProcessFBO {
            framebuffer: fb,
            color_texture: tex,
            depth_renderbuffer: rb,
            width: w,
            height: h,
        });
    }

    /// Draw the post-processing fullscreen pass.
    fn draw_postprocess_pass(&self) {
        let gl = &self.gl;
        let Some(fbo) = &self.post_fbo else { return };
        let Some(prog) = self.programs.get("postprocess") else {
            return;
        };
        let Some(mesh) = self.meshes.get(&MeshKey::Quad) else {
            return;
        };

        // Bind default framebuffer
        gl.bind_framebuffer(GL::FRAMEBUFFER, None);
        gl.viewport(0, 0, self.canvas_width as i32, self.canvas_height as i32);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT);

        gl.use_program(Some(&prog.program));

        // Bind FBO color texture as input
        gl.active_texture(GL::TEXTURE0);
        gl.bind_texture(GL::TEXTURE_2D, Some(&fbo.color_texture));
        if let Some(loc) = &prog.u_scene_texture {
            gl.uniform1i(Some(loc), 0);
        }
        // Also bind via u_texture if postprocess shader uses that name
        if let Some(loc) = &prog.u_texture {
            gl.uniform1i(Some(loc), 0);
        }

        // Set uniforms
        set_vec2(
            gl,
            &prog.u_resolution,
            self.canvas_width as f32,
            self.canvas_height as f32,
        );
        set_f32(gl, &prog.u_time, self.time);
        if let Some(loc) = &prog.u_scanline_intensity {
            gl.uniform1f(Some(loc), self.post_process.scanline_intensity);
        }
        if let Some(loc) = &prog.u_bloom_intensity {
            gl.uniform1f(Some(loc), self.post_process.bloom_intensity);
        }
        if let Some(loc) = &prog.u_vignette_intensity {
            gl.uniform1f(Some(loc), self.post_process.vignette_intensity);
        }
        if let Some(loc) = &prog.u_crt_curvature {
            gl.uniform1f(Some(loc), self.post_process.crt_curvature);
        }
        if let Some(loc) = &prog.u_grade_shadows {
            let s = &self.post_process.grade_shadows;
            gl.uniform3f(Some(loc), s[0], s[1], s[2]);
        }
        if let Some(loc) = &prog.u_grade_highlights {
            let h = &self.post_process.grade_highlights;
            gl.uniform3f(Some(loc), h[0], h[1], h[2]);
        }
        if let Some(loc) = &prog.u_grade_contrast {
            gl.uniform1f(Some(loc), self.post_process.grade_contrast);
        }
        if let Some(loc) = &prog.u_saturation {
            gl.uniform1f(Some(loc), self.post_process.saturation);
        }
        if let Some(loc) = &prog.u_chromatic_aberration {
            gl.uniform1f(Some(loc), self.post_process.chromatic_aberration);
        }
        if let Some(loc) = &prog.u_film_grain {
            gl.uniform1f(Some(loc), self.post_process.film_grain);
        }

        // Draw fullscreen quad
        gl.disable(GL::DEPTH_TEST);
        gl.disable(GL::CULL_FACE);

        gl.bind_vertex_array(Some(&mesh.vao));
        gl.draw_arrays(GL::TRIANGLES, 0, mesh.vertex_count);
        gl.bind_vertex_array(None);

        // Restore
        gl.enable(GL::DEPTH_TEST);
        gl.enable(GL::CULL_FACE);
        gl.bind_texture(GL::TEXTURE_2D, None);
    }

    /// Draw a full-screen color overlay (for damage/pickup flashes).
    /// Uses additive blending for a bright flash effect.
    pub fn draw_screen_flash(&self, color: Vec4, alpha: f32) {
        if alpha <= 0.001 {
            return;
        }
        let gl = &self.gl;
        let Some(prog) = self.programs.get("unlit") else {
            return;
        };
        let Some(mesh) = self.meshes.get(&MeshKey::Quad) else {
            return;
        };

        gl.use_program(Some(&prog.program));

        // Full-screen NDC quad: identity MVP places quad at [-0.5, 0.5] in clip space
        // Scale to fill screen: 2x2 in NDC
        let mvp = glam::Mat4::from_scale(Vec3::new(2.0, 2.0, 1.0));
        set_mat4(gl, &prog.u_mvp, &mvp);
        set_mat4(gl, &prog.u_model, &glam::Mat4::IDENTITY);
        set_vec4(
            gl,
            &prog.u_color,
            &Vec4::new(color.x, color.y, color.z, alpha),
        );
        set_f32(gl, &prog.u_fog_density, 0.0);

        // Additive blending for flash
        gl.blend_func(GL::SRC_ALPHA, GL::ONE);
        gl.disable(GL::DEPTH_TEST);
        gl.disable(GL::CULL_FACE);

        gl.bind_vertex_array(Some(&mesh.vao));
        gl.draw_arrays(GL::TRIANGLES, 0, mesh.vertex_count);
        gl.bind_vertex_array(None);

        // Restore standard blending and depth
        gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
        gl.enable(GL::DEPTH_TEST);
        gl.enable(GL::CULL_FACE);
    }

    /// Generate mesh VBOs/VAOs for each primitive type.
    fn generate_meshes(&mut self) {
        let configs: Vec<(MeshKey, Vec<f32>)> = vec![
            (MeshKey::Cuboid, generate_cuboid()),
            (MeshKey::Plane, generate_plane()),
            (MeshKey::Sphere { segments: 16 }, generate_sphere(16)),
            (MeshKey::Cylinder { segments: 16 }, generate_cylinder(16)),
            (MeshKey::Quad, generate_quad()),
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

    /// Draw all batchable sprites grouped by blend mode.
    /// A sprite is batchable if outline == 0.0 and dissolve == 0.0.
    fn draw_sprite_batches(
        &mut self,
        sorted: &[&crate::scene::RenderObject],
        vp: &Mat4,
        scene: &Scene,
        fog_density: f32,
        camera: &Camera,
    ) {
        use crate::scene::BlendMode;

        // Collect batchable sprites per blend mode
        let mut normal_sprites: Vec<&crate::scene::RenderObject> = Vec::new();
        let mut additive_sprites: Vec<&crate::scene::RenderObject> = Vec::new();
        let mut subtractive_sprites: Vec<&crate::scene::RenderObject> = Vec::new();

        for obj in sorted {
            if let MaterialType::Sprite {
                outline,
                dissolve,
                blend_mode,
                ..
            } = &obj.material
                && *outline == 0.0
                && *dissolve == 0.0
            {
                match blend_mode {
                    BlendMode::Normal => normal_sprites.push(obj),
                    BlendMode::Additive => additive_sprites.push(obj),
                    BlendMode::Subtractive => subtractive_sprites.push(obj),
                }
            }
        }

        if normal_sprites.is_empty()
            && additive_sprites.is_empty()
            && subtractive_sprites.is_empty()
        {
            return;
        }

        // Build all batch vertex data first (needs &mut self)
        // We'll store them in temporary Vecs to avoid borrow issues.
        let mut normal_verts: Vec<f32> = Vec::new();
        let mut additive_verts: Vec<f32> = Vec::new();
        let mut subtractive_verts: Vec<f32> = Vec::new();
        if !normal_sprites.is_empty() {
            self.build_sprite_batch(&normal_sprites);
            std::mem::swap(&mut normal_verts, &mut self.batch_vertices);
        }
        if !additive_sprites.is_empty() {
            self.build_sprite_batch(&additive_sprites);
            std::mem::swap(&mut additive_verts, &mut self.batch_vertices);
        }
        if !subtractive_sprites.is_empty() {
            self.build_sprite_batch(&subtractive_sprites);
            std::mem::swap(&mut subtractive_verts, &mut self.batch_vertices);
        }

        let gl = &self.gl;
        let Some(prog) = self.programs.get("sprite_batch") else {
            return;
        };
        gl.use_program(Some(&prog.program));

        // Set view-projection matrix (u_vp)
        if let Some(loc) = &prog.u_mvp {
            gl.uniform_matrix4fv_with_f32_array(Some(loc), false, vp.as_ref());
        }
        set_f32(gl, &prog.u_fog_density, fog_density);
        set_vec3(gl, &prog.u_camera_pos, &camera.position);

        // Bind atlas texture (all sprites use atlas 0)
        gl.active_texture(GL::TEXTURE0);
        if let Some(tex) = self.atlases.get(&0) {
            gl.bind_texture(GL::TEXTURE_2D, Some(tex));
        }
        if let Some(loc) = &prog.u_texture {
            gl.uniform1i(Some(loc), 0);
        }

        // Set lighting uniforms (scene-global)
        let light_count = scene.lighting.lights.len().min(32) as i32;
        if let Some(loc) = &prog.u_light_count {
            gl.uniform1i(Some(loc), light_count);
        }
        set_f32(gl, &prog.u_ambient, scene.lighting.ambient);
        if let Some(loc) = &prog.u_ambient_color {
            let ac = &scene.lighting.ambient_color;
            gl.uniform3f(Some(loc), ac[0], ac[1], ac[2]);
        }
        if let Some(loc) = &prog.u_ramp_shadow {
            let rs = &scene.lighting.ramp_shadow;
            gl.uniform3f(Some(loc), rs[0], rs[1], rs[2]);
        }
        if let Some(loc) = &prog.u_ramp_mid {
            let rm = &scene.lighting.ramp_mid;
            gl.uniform3f(Some(loc), rm[0], rm[1], rm[2]);
        }
        if let Some(loc) = &prog.u_ramp_highlight {
            let rh = &scene.lighting.ramp_highlight;
            gl.uniform3f(Some(loc), rh[0], rh[1], rh[2]);
        }
        set_f32(gl, &prog.u_posterize, scene.lighting.posterize);
        if let Some(loc) = &prog.u_fog_color {
            let fc = &scene.lighting.fog_color;
            gl.uniform3f(Some(loc), fc[0], fc[1], fc[2]);
        }
        for (i, light) in scene.lighting.lights.iter().take(32).enumerate() {
            if let Some(loc) = prog.u_lights.get(i).and_then(|l| l.as_ref()) {
                gl.uniform4f(Some(loc), light[0], light[1], light[2], light[3]);
            }
            if let Some(loc) = prog.u_light_color.get(i).and_then(|l| l.as_ref()) {
                let c = scene
                    .lighting
                    .light_colors
                    .get(i)
                    .copied()
                    .unwrap_or([1.0, 1.0, 1.0, 0.0]);
                gl.uniform4f(Some(loc), c[0], c[1], c[2], c[3]);
            }
        }

        gl.disable(GL::CULL_FACE);

        // Draw Normal blend batch
        if !normal_verts.is_empty() {
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            gl.blend_equation(GL::FUNC_ADD);
            self.upload_and_draw_batch_data(&normal_verts);
        }

        // Draw Additive blend batch
        if !additive_verts.is_empty() {
            gl.blend_func(GL::SRC_ALPHA, GL::ONE);
            gl.blend_equation(GL::FUNC_ADD);
            self.upload_and_draw_batch_data(&additive_verts);
        }

        // Draw Subtractive blend batch
        if !subtractive_verts.is_empty() {
            gl.blend_func(GL::SRC_ALPHA, GL::ONE);
            gl.blend_equation(GL::FUNC_REVERSE_SUBTRACT);
            self.upload_and_draw_batch_data(&subtractive_verts);
        }

        // Restore default blend state
        gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
        gl.blend_equation(GL::FUNC_ADD);
        gl.enable(GL::CULL_FACE);
    }

    /// Ensure the batch VAO/VBO is created (lazy init).
    fn ensure_batch_vao(&mut self) {
        if self.batch_vao.is_some() {
            return;
        }
        let gl = &self.gl;
        let vao = match gl.create_vertex_array() {
            Some(v) => v,
            None => return,
        };
        let vbo = match gl.create_buffer() {
            Some(b) => b,
            None => return,
        };
        gl.bind_vertex_array(Some(&vao));
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));

        // Batch vertex layout: pos(3) + uv(2) + tint(4) = 9 floats = 36 bytes
        let stride = 9 * 4;
        // location 0: position (vec3)
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 3, GL::FLOAT, false, stride, 0);
        // location 1: uv (vec2)
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(1, 2, GL::FLOAT, false, stride, 12);
        // location 2: tint (vec4)
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_with_i32(2, 4, GL::FLOAT, false, stride, 20);

        gl.bind_vertex_array(None);
        self.batch_vao = Some(vao);
        self.batch_vbo = Some(vbo);
    }

    /// Build batch vertex data for a set of batchable sprites.
    /// Each sprite becomes 6 vertices (2 triangles), with pre-computed
    /// world-space positions and atlas UVs.
    fn build_sprite_batch(&mut self, sprites: &[&crate::scene::RenderObject]) {
        self.batch_vertices.clear();
        for obj in sprites {
            let MaterialType::Sprite {
                sprite_rect,
                tint,
                flip_x,
                ..
            } = &obj.material
            else {
                continue;
            };
            // Pre-compute world-space quad corners from transform
            let t = &obj.transform;
            let half_x = t.scale.x * 0.5;
            let half_y = t.scale.y * 0.5;
            let cx = t.translation.x;
            let cy = t.translation.y;
            let z = t.translation.z;

            // Quad corners (no rotation for 2D sprites)
            let x0 = cx - half_x;
            let x1 = cx + half_x;
            let y0 = cy - half_y;
            let y1 = cy + half_y;

            // Pre-compute atlas UVs from sprite_rect + flip
            let (u0, u1) = if *flip_x {
                (sprite_rect.z, sprite_rect.x) // reversed
            } else {
                (sprite_rect.x, sprite_rect.z)
            };
            // V is inverted: sprite_rect.w = v_bottom, sprite_rect.y = v_top
            let v0 = sprite_rect.w; // bottom
            let v1 = sprite_rect.y; // top

            let tr = tint.x;
            let tg = tint.y;
            let tb = tint.z;
            let ta = tint.w;

            // Triangle 1: bottom-left, top-right, bottom-right
            self.batch_vertices
                .extend_from_slice(&[x0, y0, z, u0, v0, tr, tg, tb, ta]);
            self.batch_vertices
                .extend_from_slice(&[x1, y1, z, u1, v1, tr, tg, tb, ta]);
            self.batch_vertices
                .extend_from_slice(&[x1, y0, z, u1, v0, tr, tg, tb, ta]);
            // Triangle 2: bottom-left, top-left, top-right
            self.batch_vertices
                .extend_from_slice(&[x0, y0, z, u0, v0, tr, tg, tb, ta]);
            self.batch_vertices
                .extend_from_slice(&[x0, y1, z, u0, v1, tr, tg, tb, ta]);
            self.batch_vertices
                .extend_from_slice(&[x1, y1, z, u1, v1, tr, tg, tb, ta]);
        }
    }

    /// Upload batch vertex data and draw. Uses GL handle directly to avoid self borrow issues.
    fn upload_and_draw_batch_data(&self, data: &[f32]) {
        if data.is_empty() {
            return;
        }
        let gl = &self.gl;
        let Some(vao) = &self.batch_vao else { return };
        let Some(vbo) = &self.batch_vbo else { return };

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(vbo));

        // Always use buffer_data (simpler than tracking capacity for immutable self)
        unsafe {
            let view = js_sys::Float32Array::view(data);
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, GL::DYNAMIC_DRAW);
        }

        let vertex_count = (data.len() / 9) as i32;
        gl.draw_arrays(GL::TRIANGLES, 0, vertex_count);
        gl.bind_vertex_array(None);
    }
}

// --- Uniform helpers ---

fn set_mat4(gl: &GL, loc: &Option<WebGlUniformLocation>, m: &Mat4) {
    if let Some(loc) = loc {
        gl.uniform_matrix4fv_with_f32_array(Some(loc), false, m.as_ref());
    }
}

/// Sort key for grouping objects by shader program (minimizes program switches).
/// Parallax renders first (background), then world geometry, sprites, VFX, fog, HUD.
/// Wide gaps allow future layer insertion without renumbering.
fn material_sort_key(m: &MaterialType) -> u8 {
    match m {
        MaterialType::Parallax { .. } => 0,
        MaterialType::Unlit { .. } => 10,
        MaterialType::Gradient { .. } => 15,
        MaterialType::Water { .. } => 20,
        MaterialType::Sprite { .. } => 30,
        MaterialType::Glow { .. } => 35,
        MaterialType::Ripple { .. } => 40,
        MaterialType::TronWall { .. } => 45,
        MaterialType::WhipTrail { .. } => 50,
        MaterialType::SlashArc { .. } => 52,
        MaterialType::MagicCircle { .. } => 54,
        MaterialType::GodRays { .. } => 56,
        MaterialType::FogLayer { .. } => 60,
        MaterialType::HealthBar { .. } => 70,
    }
}

fn set_vec2(gl: &GL, loc: &Option<WebGlUniformLocation>, x: f32, y: f32) {
    if let Some(loc) = loc {
        gl.uniform2f(Some(loc), x, y);
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

// --- Frustum culling ---

/// Six frustum planes extracted from a view-projection matrix.
/// Each plane is (a, b, c, d) where ax + by + cz + d = 0.
type FrustumPlanes = [Vec4; 6];

/// Extract frustum planes from a view-projection matrix using the
/// Gribb/Hartmann method. Planes point inward (positive = inside).
fn extract_frustum_planes(vp: &Mat4) -> FrustumPlanes {
    let r0 = vp.row(0);
    let r1 = vp.row(1);
    let r2 = vp.row(2);
    let r3 = vp.row(3);
    [
        r3 + r0, // left
        r3 - r0, // right
        r3 + r1, // bottom
        r3 - r1, // top
        r3 + r2, // near
        r3 - r2, // far
    ]
}

/// Test whether a bounding sphere is at least partially inside the frustum.
fn is_sphere_in_frustum(planes: &FrustumPlanes, center: Vec3, radius: f32) -> bool {
    for plane in planes {
        let dist = plane.x * center.x + plane.y * center.y + plane.z * center.z + plane.w;
        let norm = (plane.x * plane.x + plane.y * plane.y + plane.z * plane.z).sqrt();
        if norm > 0.0 && dist < -radius * norm {
            return false;
        }
    }
    true
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
    let verts_per_quad = 6;
    let total_verts = segments as usize * segments as usize * verts_per_quad;
    let mut buf = Vec::with_capacity(total_verts * 8);
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
    // 6 verts for side quad + 3 top cap + 3 bottom cap = 12 per segment
    let mut buf = Vec::with_capacity(segments as usize * 12 * 8);
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

/// Unit quad on the XY plane at Z=0, facing +Z (toward the side-view camera at Z<0).
fn generate_quad() -> Vec<f32> {
    let mut buf = Vec::with_capacity(6 * 8);
    let normal = Vec3::Z;
    let h = 0.5;
    // Quad corners on XY plane — wound CCW when viewed from +Z
    let v00 = Vec3::new(-h, -h, 0.0);
    let v10 = Vec3::new(h, -h, 0.0);
    let v11 = Vec3::new(h, h, 0.0);
    let v01 = Vec3::new(-h, h, 0.0);
    push_vertex(&mut buf, v00, normal, 0.0, 0.0);
    push_vertex(&mut buf, v11, normal, 1.0, 1.0);
    push_vertex(&mut buf, v10, normal, 1.0, 0.0);
    push_vertex(&mut buf, v00, normal, 0.0, 0.0);
    push_vertex(&mut buf, v01, normal, 0.0, 1.0);
    push_vertex(&mut buf, v11, normal, 1.0, 1.0);
    buf
}
