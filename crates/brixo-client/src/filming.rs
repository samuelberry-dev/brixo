//! Switches for filming Brixo (all inert unless set). See tools/film.py.

/// BRIXO_WINDOW_SIZE ("1920x1080"): a borderless window of exactly this
/// many pixels at the top-left corner, so a screen recorder captures
/// precisely the app.
pub fn window_size() -> Option<(u32, u32)> {
    let text = std::env::var("BRIXO_WINDOW_SIZE").ok()?;
    let (w, h) = text.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// BRIXO_WINDOW_TITLE: a fixed window title (so a recorder can find it).
pub fn window_title() -> Option<String> {
    std::env::var("BRIXO_WINDOW_TITLE").ok()
}

/// Window settings with the filming switches applied.
pub fn window_attributes(title: &str, w: f64, h: f64) -> winit::window::WindowAttributes {
    let attrs = winit::window::Window::default_attributes().with_title(window_title().unwrap_or_else(|| title.to_string()));
    match window_size() {
        // Always on top, so nothing (a terminal) covers it mid-recording.
        Some((fw, fh)) => attrs
            .with_inner_size(winit::dpi::PhysicalSize::new(fw, fh))
            .with_position(winit::dpi::PhysicalPosition::new(0, 0))
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop),
        None => attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h)),
    }
}
