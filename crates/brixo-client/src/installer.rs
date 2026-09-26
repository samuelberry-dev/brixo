//! The installing window: the Brixo logo on the website's navy stud
//! banner, with a row of bricks dropping into place as each step finishes.
//! Uninstalling uses the same window.
//!
//! It has its own little event loop, and a program only gets one: so it
//! runs only when this process is about to hand over to another (the
//! installed copy) or quit, never before the app's own window.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// One thing to do, with what to say while it's happening.
pub struct Step {
    pub label: String,
    pub run: Box<dyn FnOnce() -> Result<(), String> + Send>,
}

impl Step {
    pub fn new(label: impl Into<String>, run: impl FnOnce() -> Result<(), String> + Send + 'static) -> Step {
        Step { label: label.into(), run: Box::new(run) }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Failed(String),
}

/// Each step shows for at least this long, so the bricks can be seen landing.
const MIN_STEP: Duration = Duration::from_millis(450);
/// "Ready!" stays up this long before the window closes by itself.
const HOLD_DONE: Duration = Duration::from_millis(1100);
const SLOTS: usize = 10;
const SIZE: (f64, f64) = (520.0, 320.0);

// The logo's colours, the same as the website's.
const BRICKS: [(char, [u8; 3]); 5] =
    [('B', [196, 40, 28]), ('R', [13, 105, 172]), ('I', [224, 168, 0]), ('X', [75, 151, 75]), ('O', [218, 133, 65])];

#[derive(Default)]
struct Progress {
    done_steps: usize,
    label: String,
    result: Option<Result<(), String>>,
    finished_at: Option<Instant>,
}

/// Shows the window, runs `steps` in order on a background thread, and
/// returns once it has closed. `subtitle` goes under the logo ("Brixo
/// Player"); `done_text` replaces the status at the end ("Ready!").
pub fn run(subtitle: &str, steps: Vec<Step>, done_text: &str) -> Outcome {
    let total = steps.len().max(1);
    let progress = Arc::new(Mutex::new(Progress::default()));
    let Ok(event_loop) = EventLoop::new() else {
        // No window possible: just wait for the steps.
        start_steps(steps, progress.clone());
        loop {
            if let Some(r) = progress.lock().unwrap().result.clone() {
                return match r {
                    Ok(()) => Outcome::Done,
                    Err(e) => Outcome::Failed(e),
                };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut ui = InstallerUi {
        pending: Some(steps),
        gpu: None,
        subtitle: subtitle.to_string(),
        done_text: done_text.to_string(),
        total,
        progress,
        landed: Vec::new(),
        started: Instant::now(),
        outcome: None,
        pointer_on_button: false,
    };
    let _ = event_loop.run_app(&mut ui);
    ui.outcome.unwrap_or(Outcome::Failed("the installer window was closed".into()))
}

/// Runs the steps in order on their own thread, reporting into `progress`.
fn start_steps(steps: Vec<Step>, progress: Arc<Mutex<Progress>>) {
    std::thread::spawn(move || {
        for step in steps {
            progress.lock().unwrap().label = step.label.clone();
            let started = Instant::now();
            let result = (step.run)();
            if let Some(rest) = MIN_STEP.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
            let mut p = progress.lock().unwrap();
            if let Err(e) = result {
                p.result = Some(Err(e));
                p.finished_at = Some(Instant::now());
                return;
            }
            p.done_steps += 1;
        }
        let mut p = progress.lock().unwrap();
        p.result = Some(Ok(()));
        p.finished_at = Some(Instant::now());
    });
}

struct InstallerUi {
    /// The steps, until the window is up (so you see every one happen).
    pending: Option<Vec<Step>>,
    gpu: Option<MiniGpu>,
    subtitle: String,
    done_text: String,
    total: usize,
    progress: Arc<Mutex<Progress>>,
    /// When each progress brick landed.
    landed: Vec<Instant>,
    started: Instant,
    outcome: Option<Outcome>,
    pointer_on_button: bool,
}

impl ApplicationHandler for InstallerUi {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title(format!("Installing {}", self.subtitle))
            .with_inner_size(winit::dpi::LogicalSize::new(SIZE.0, SIZE.1))
            .with_decorations(false)
            .with_resizable(false)
            .with_window_icon(Some(crate::filming::brixo_icon()));
        // In the middle of the main screen.
        if let Some(monitor) = event_loop.primary_monitor() {
            let (m, scale) = (monitor.size(), monitor.scale_factor());
            let (w, h) = (SIZE.0 * scale, SIZE.1 * scale);
            let pos = monitor.position();
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                pos.x + ((m.width as f64 - w) / 2.0) as i32,
                pos.y + ((m.height as f64 - h) / 2.0) as i32,
            ));
        }
        // If it can't be drawn, the steps still run (see about_to_wait).
        if let Ok(window) = event_loop.create_window(attrs) {
            self.gpu = MiniGpu::new(Arc::new(window));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        let _ = gpu.egui_state.on_window_event(&gpu.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                // Only once it's finished: stopping halfway would leave a mess.
                if self.progress.lock().unwrap().result.is_some() {
                    self.finish(event_loop);
                }
            }
            WindowEvent::Resized(size) => gpu.resize(size.width, size.height),
            // No title bar: drag the window by anything that isn't a button.
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if !self.pointer_on_button => {
                let _ = gpu.window.drag_window();
            }
            WindowEvent::RedrawRequested => {
                if self.frame() {
                    self.finish(event_loop);
                    return;
                }
                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match self.gpu.as_ref() {
            Some(gpu) => gpu.window.request_redraw(),
            // No window (it couldn't be drawn): finish when the steps do.
            None => {
                if let Some(steps) = self.pending.take() {
                    start_steps(steps, self.progress.clone());
                }
                if self.progress.lock().unwrap().result.is_some() {
                    self.finish(event_loop);
                } else {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

impl InstallerUi {
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        let result = self.progress.lock().unwrap().result.clone();
        self.outcome = Some(match result {
            Some(Ok(())) => Outcome::Done,
            Some(Err(e)) => Outcome::Failed(e),
            None => Outcome::Failed("stopped before it finished".into()),
        });
        event_loop.exit();
    }

    /// Draws one frame. True when it's time to close.
    fn frame(&mut self) -> bool {
        let (done_steps, label, result, finished_at) = {
            let p = self.progress.lock().unwrap();
            (p.done_steps, p.label.clone(), p.result.clone(), p.finished_at)
        };
        // Bricks land as steps finish (all of them once it's done).
        let target = if matches!(result, Some(Ok(()))) { SLOTS } else { done_steps * SLOTS / self.total };
        // One at a time, a little apart, however many are due.
        let spacing = Duration::from_millis(70);
        if self.landed.len() < target && self.landed.last().is_none_or(|t| t.elapsed() >= spacing) {
            self.landed.push(Instant::now());
        }
        let all_landed = self.landed.len() >= SLOTS && self.landed.last().is_some_and(|t| t.elapsed() > Duration::from_millis(250));
        let close_now = matches!(result, Some(Ok(()))) && all_landed && finished_at.is_some_and(|t| t.elapsed() >= HOLD_DONE);

        let Some(gpu) = self.gpu.as_mut() else { return close_now };
        let raw = gpu.egui_state.take_egui_input(&gpu.window);
        let mut close_clicked = false;
        let mut on_button = false;
        let (subtitle, done_text, landed, t) = (&self.subtitle, &self.done_text, &self.landed, self.started.elapsed().as_secs_f32());
        let out = gpu.egui_ctx.run(raw, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                let rect = ui.max_rect();
                let p = ui.painter().clone();
                paint_background(&p, rect);
                paint_logo(&p, rect, t);
                p.text(
                    egui::pos2(rect.center().x, rect.top() + 150.0),
                    egui::Align2::CENTER_CENTER,
                    subtitle,
                    egui::FontId::proportional(20.0),
                    egui::Color32::from_rgb(207, 227, 247),
                );
                paint_slots(&p, rect, landed);
                let (status, color) = match &result {
                    Some(Ok(())) => (done_text.clone(), egui::Color32::from_rgb(245, 205, 48)),
                    Some(Err(e)) => (format!("Something went wrong: {e}"), egui::Color32::from_rgb(255, 150, 140)),
                    None if label.is_empty() => ("Getting ready...".to_string(), egui::Color32::WHITE),
                    None => (label.clone(), egui::Color32::WHITE),
                };
                let status_rect = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 262.0), egui::vec2(460.0, 40.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(status_rect), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(status).size(16.0).color(color));
                    });
                });
                if matches!(result, Some(Err(_))) {
                    let r = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 22.0), egui::vec2(100.0, 26.0));
                    let button = ui.put(r, egui::Button::new("Close"));
                    on_button = button.hovered();
                    close_clicked = button.clicked();
                }
                p.text(
                    egui::pos2(rect.right() - 10.0, rect.bottom() - 8.0),
                    egui::Align2::RIGHT_BOTTOM,
                    crate::install::VERSION,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 70),
                );
            });
        });
        self.pointer_on_button = on_button;
        gpu.egui_state.handle_platform_output(&gpu.window, out.platform_output.clone());
        gpu.draw(out);
        if let Some(steps) = self.pending.take() {
            start_steps(steps, self.progress.clone());
        }
        close_now || close_clicked
    }
}

fn rgb(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

fn shade(c: [u8; 3], f: f32) -> egui::Color32 {
    let s = |v: u8| ((v as f32 * f).round().clamp(0.0, 255.0)) as u8;
    egui::Color32::from_rgb(s(c[0]), s(c[1]), s(c[2]))
}

/// The website banner: navy, lighter at the top, with a grid of studs.
fn paint_background(p: &egui::Painter, rect: egui::Rect) {
    let (top, bottom) = (egui::Color32::from_rgb(42, 90, 146), egui::Color32::from_rgb(13, 42, 74));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    p.add(egui::Shape::mesh(mesh));
    let mut y = rect.top() + 12.0;
    while y < rect.bottom() {
        let mut x = rect.left() + 12.0;
        while x < rect.right() {
            p.circle_filled(egui::pos2(x, y + 0.8), 5.5, egui::Color32::from_black_alpha(45));
            p.circle_filled(egui::pos2(x, y), 5.0, egui::Color32::from_white_alpha(22));
            x += 24.0;
        }
        y += 24.0;
    }
    p.rect_stroke(rect.shrink(0.5), 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(10, 31, 56)), egui::StrokeKind::Inside);
}

/// One brick: studs on top, a lighter top edge, a darker bottom edge.
fn paint_brick(p: &egui::Painter, r: egui::Rect, color: [u8; 3], studs: usize) {
    let stud_w = (r.width() * 0.26).min(18.0);
    let stud_h = (r.height() * 0.16).max(3.0);
    for i in 0..studs {
        let cx = r.left() + r.width() * (i as f32 + 0.5) / studs as f32;
        let s = egui::Rect::from_min_size(egui::pos2(cx - stud_w / 2.0, r.top() - stud_h + 1.0), egui::vec2(stud_w, stud_h));
        p.rect_filled(s, 2.0, rgb(color));
        p.rect_filled(egui::Rect::from_min_size(s.min, egui::vec2(stud_w, stud_h * 0.45)), 2.0, shade(color, 1.18));
    }
    p.rect_filled(r.translate(egui::vec2(0.0, 2.5)), 5.0, egui::Color32::from_black_alpha(70));
    p.rect_filled(r, 5.0, shade(color, 0.78));
    p.rect_filled(egui::Rect::from_min_max(r.min, egui::pos2(r.right(), r.bottom() - 3.0)), 5.0, rgb(color));
    p.rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(r.width(), 4.0)), 3.0, shade(color, 1.15));
}

/// B R I X O in bricks, bobbing gently like a staircase.
fn paint_logo(p: &egui::Painter, rect: egui::Rect, t: f32) {
    let (w, h, gap) = (64.0, 56.0, 8.0);
    let x0 = rect.center().x - (5.0 * w + 4.0 * gap) / 2.0;
    for (i, (letter, color)) in BRICKS.iter().enumerate() {
        let bob = (t * 2.2 - i as f32 * 0.55).sin() * 3.0 + if i % 2 == 1 { 3.0 } else { 0.0 };
        let r = egui::Rect::from_min_size(egui::pos2(x0 + i as f32 * (w + gap), rect.top() + 58.0 + bob), egui::vec2(w, h));
        paint_brick(p, r, *color, 2);
        let c = r.center() + egui::vec2(0.0, 1.0);
        let font = egui::FontId::proportional(40.0);
        p.text(c + egui::vec2(2.0, 2.5), egui::Align2::CENTER_CENTER, letter, font.clone(), egui::Color32::from_black_alpha(90));
        // The font has no bold: a few copies a fraction of a pixel apart fatten it.
        for d in [egui::vec2(0.0, 0.0), egui::vec2(1.0, 0.0), egui::vec2(0.5, 0.6), egui::vec2(0.5, -0.4)] {
            p.text(c + d, egui::Align2::CENTER_CENTER, letter, font.clone(), egui::Color32::WHITE);
        }
    }
}

/// The progress bar: empty sockets, and bricks that drop into them.
fn paint_slots(p: &egui::Painter, rect: egui::Rect, landed: &[Instant]) {
    let (w, h, gap) = (36.0, 18.0, 6.0);
    let x0 = rect.center().x - (SLOTS as f32 * w + (SLOTS - 1) as f32 * gap) / 2.0;
    let y = rect.top() + 206.0;
    for i in 0..SLOTS {
        let slot = egui::Rect::from_min_size(egui::pos2(x0 + i as f32 * (w + gap), y), egui::vec2(w, h));
        p.rect_filled(slot, 4.0, egui::Color32::from_black_alpha(60));
        p.rect_stroke(slot, 4.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
        if let Some(at) = landed.get(i) {
            // Falls in from above and settles with a small bounce.
            let k = (at.elapsed().as_secs_f32() / 0.28).min(1.0);
            let fall = (1.0 - k) * (1.0 - k) * -34.0;
            let bounce = if k >= 1.0 { 0.0 } else { (k * std::f32::consts::PI).sin() * -2.0 * (1.0 - k) };
            let color = BRICKS[i % BRICKS.len()].1;
            paint_brick(p, slot.translate(egui::vec2(0.0, fall + bounce)), color, 2);
        }
    }
}

/// Just enough graphics to draw the window.
struct MiniGpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

impl MiniGpu {
    fn new(window: Arc<Window>) -> Option<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).ok()?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).ok()?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(*caps.formats.first()?);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let egui_ctx = egui::Context::default();
        crate::theme::apply(&egui_ctx);
        let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &window, None, None, None);
        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);
        Some(Self { window, surface, device, queue, config, egui_ctx, egui_state, egui_renderer })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn draw(&mut self, out: egui::FullOutput) {
        let ppp = self.egui_ctx.pixels_per_point();
        let jobs = self.egui_ctx.tessellate(out.shapes, ppp);
        let Ok(frame) = self.surface.get_current_texture() else {
            self.surface.configure(&self.device, &self.config);
            return;
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("installer") });
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point: ppp };
        for (id, delta) in &out.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }
        self.egui_renderer.update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("installer pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.005, g: 0.02, b: 0.06, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            self.egui_renderer.render(&mut pass, &jobs, &screen);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        for id in &out.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}
