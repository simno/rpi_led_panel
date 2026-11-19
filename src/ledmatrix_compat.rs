//! A compatibility layer for the API of the [rpi-led-matrix](https://docs.rs/rpi-led-matrix) crate, so that
//! code written for it can run on this library without the C++ dependency.
//!
//! Errors are `String`s instead of `&'static str`, and some options that this library has no equivalent for
//! are accepted but ignored, as documented on their setters.

use std::{path::Path, str::FromStr};

use crate::{BdfFont, Canvas, NamedPixelMapperType, RGBMatrix, RGBMatrixConfig};

/// LED color representation (red, green, blue)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LedColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl LedColor {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        LedColor { red, green, blue }
    }
}

/// A BDF font for [`LedCanvas::draw_text`].
#[derive(Clone, Debug)]
pub struct LedFont {
    font: BdfFont,
}

impl LedFont {
    pub fn new(path: &Path) -> Result<Self, String> {
        let font = BdfFont::load(path).map_err(|e| format!("Failed to load font: {e}"))?;
        Ok(LedFont { font })
    }

    /// The width of the text when drawn with [`LedCanvas::draw_text`] and no kerning offset.
    pub fn measure_text_width(&self, text: &str) -> i32 {
        self.font.text_width(text) as i32
    }
}

/// A frame to draw on, which is shown by passing it to [`LedMatrix::swap`].
pub struct LedCanvas {
    canvas: Canvas,
}

impl LedCanvas {
    /// Retrieves the width & height of the canvas
    pub fn canvas_size(&self) -> (i32, i32) {
        (self.width(), self.height())
    }

    /// Get the width of the canvas
    pub fn width(&self) -> i32 {
        self.canvas.width() as i32
    }

    /// Get the height of the canvas
    pub fn height(&self) -> i32 {
        self.canvas.height() as i32
    }

    /// Fill the entire canvas with a color
    pub fn fill(&mut self, color: &LedColor) {
        self.canvas.fill(color.red, color.green, color.blue);
    }

    /// Set a single pixel. Pixels outside of the canvas are ignored.
    pub fn set(&mut self, x: i32, y: i32, color: &LedColor) {
        if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
            self.canvas
                .set_pixel(x, y, color.red, color.green, color.blue);
        }
    }

    /// Draw text with its baseline at `y`, adding `kerning_offset` pixels after each character. With
    /// `vertical`, the characters go from top to bottom instead of left to right.
    ///
    /// Returns how far the text advanced, in pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        font: &LedFont,
        text: &str,
        x: i32,
        y: i32,
        color: &LedColor,
        kerning_offset: i32,
        vertical: bool,
    ) -> i32 {
        let LedColor { red, green, blue } = *color;
        if vertical {
            self.canvas
                .draw_text_vertical(&font.font, x, y, red, green, blue, text, kerning_offset)
        } else {
            self.canvas
                .draw_text(&font.font, x, y, red, green, blue, text, kerning_offset)
        }
    }

    /// Draw a filled circle.
    ///
    /// Note that this differs from rpi-led-matrix, which only draws the outline.
    pub fn draw_circle(&mut self, x: i32, y: i32, radius: i32, color: &LedColor) {
        self.canvas
            .draw_circle(x, y, radius, color.red, color.green, color.blue);
    }
}

/// Configuration options for the LED matrix
#[derive(Clone, Debug)]
pub struct LedMatrixOptions {
    rows: usize,
    cols: usize,
    chain_length: usize,
    parallel: usize,
    pixel_mapper_config: String,
    pwm_lsb_nanoseconds: u32,
    refresh_rate: usize,
    limit_refresh: u32,
    dither_bits: usize,
    pwm_bits: usize,
}

impl LedMatrixOptions {
    pub fn new() -> Self {
        LedMatrixOptions {
            rows: 64,
            cols: 128,
            chain_length: 2,
            parallel: 1,
            pixel_mapper_config: String::new(),
            pwm_lsb_nanoseconds: 130,
            refresh_rate: 120,
            limit_refresh: 0,
            dither_bits: 0,
            pwm_bits: 11,
        }
    }

    pub fn set_rows(&mut self, rows: i32) {
        self.rows = rows as usize;
    }

    pub fn set_cols(&mut self, cols: i32) {
        self.cols = cols as usize;
    }

    pub fn set_chain_length(&mut self, chain_length: i32) {
        self.chain_length = chain_length as usize;
    }

    pub fn set_parallel(&mut self, parallel: i32) {
        self.parallel = parallel as usize;
    }

    /// A semicolon separated list of pixel mappers, e.g. `"U-mapper;Rotate:90"`.
    pub fn set_pixel_mapper_config(&mut self, config: &str) {
        self.pixel_mapper_config = config.to_string();
    }

    /// Ignored: this library always uses hardware pulsing.
    pub fn set_hardware_pulsing(&mut self, _pulsing: bool) {}

    pub fn set_pwm_lsb_nanoseconds(&mut self, ns: i32) {
        self.pwm_lsb_nanoseconds = ns as u32;
    }

    /// Set the refresh rate in Hz (e.g. 30, 50, 60, 120).
    ///
    /// Note that this differs from rpi-led-matrix, where this switches printing the refresh rate on or off.
    pub fn set_refresh_rate(&mut self, refresh_hz: usize) {
        self.refresh_rate = refresh_hz;
    }

    /// Limit the refresh rate to this many Hz, where 0 means no limit. A limit takes precedence over
    /// [`LedMatrixOptions::set_refresh_rate`].
    pub fn set_limit_refresh(&mut self, limit: i32) {
        self.limit_refresh = limit.max(0) as u32;
    }

    pub fn set_dither_bits(&mut self, bits: usize) {
        self.dither_bits = bits;
    }

    /// Same as [`LedMatrixOptions::set_dither_bits`], under the name used by rpi-led-matrix.
    pub fn set_pwm_dither_bits(&mut self, bits: u32) {
        self.dither_bits = bits as usize;
    }

    pub fn set_pwm_bits(&mut self, bits: usize) {
        self.pwm_bits = bits;
    }

    fn to_rgb_matrix_config(
        &self,
        runtime: Option<&LedRuntimeOptions>,
    ) -> Result<RGBMatrixConfig, String> {
        let mut config = RGBMatrixConfig {
            rows: self.rows,
            cols: self.cols,
            chain_length: self.chain_length,
            parallel: self.parallel,
            pwm_lsb_nanoseconds: self.pwm_lsb_nanoseconds,
            refresh_rate: self.refresh_rate,
            dither_bits: self.dither_bits,
            pwm_bits: self.pwm_bits,
            ..RGBMatrixConfig::default()
        };

        if self.limit_refresh > 0 {
            config.refresh_rate = self.limit_refresh as usize;
        }

        if let Some(runtime) = runtime {
            config.slowdown = Some(runtime.gpio_slowdown);
        }

        for mapper in self
            .pixel_mapper_config
            .split(';')
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            let mapper = NamedPixelMapperType::from_str(mapper)
                .map_err(|e| format!("Failed to parse pixel mapper config: {e}"))?;
            config.pixelmapper.push(mapper);
        }

        Ok(config)
    }
}

impl Default for LedMatrixOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime options for the LED matrix
#[derive(Clone, Debug)]
pub struct LedRuntimeOptions {
    gpio_slowdown: u32,
}

impl LedRuntimeOptions {
    pub fn new() -> Self {
        LedRuntimeOptions { gpio_slowdown: 1 }
    }

    pub fn set_gpio_slowdown(&mut self, slowdown: i32) {
        self.gpio_slowdown = slowdown.max(0) as u32;
    }
}

impl Default for LedRuntimeOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// LED matrix controller with double buffering.
///
/// Draw on the canvas from [`LedMatrix::offscreen_canvas`] and show it with [`LedMatrix::swap`], which returns
/// the canvas for the next frame:
///
/// ```ignore
/// let mut canvas = matrix.offscreen_canvas();
/// loop {
///     canvas.fill(&LedColor::new(255, 0, 0));
///     canvas = matrix.swap(canvas);
/// }
/// ```
pub struct LedMatrix {
    matrix: RGBMatrix,
    /// A cleared canvas, to hand out when the canvas from `RGBMatrix::new` has already been taken.
    blank_canvas: Canvas,
    /// The canvas from `RGBMatrix::new`, until it is taken.
    initial_canvas: Option<Canvas>,
}

impl LedMatrix {
    /// Create a new LED matrix with the given options
    pub fn new(
        options: Option<LedMatrixOptions>,
        runtime: Option<LedRuntimeOptions>,
    ) -> Result<Self, String> {
        let config = options
            .unwrap_or_default()
            .to_rgb_matrix_config(runtime.as_ref())?;

        let (matrix, canvas) =
            RGBMatrix::new(config).map_err(|e| format!("Failed to create RGB matrix: {e}"))?;

        log::info!(
            "LED matrix created with a {}x{} canvas",
            canvas.width(),
            canvas.height()
        );

        Ok(LedMatrix {
            matrix,
            blank_canvas: canvas.clone(),
            initial_canvas: Some(canvas),
        })
    }

    /// Get a cleared canvas to draw on, which is shown by passing it to [`LedMatrix::swap`].
    pub fn offscreen_canvas(&mut self) -> LedCanvas {
        let canvas = self
            .initial_canvas
            .take()
            .unwrap_or_else(|| self.blank_canvas.clone());
        LedCanvas { canvas }
    }

    /// Same as [`LedMatrix::offscreen_canvas`]. This library can't draw directly on the displayed frame, so
    /// nothing drawn on it is visible until it is passed to [`LedMatrix::swap`].
    pub fn canvas(&mut self) -> LedCanvas {
        self.offscreen_canvas()
    }

    /// Show the canvas, blocking until the end of the current frame. Returns a cleared canvas for the next
    /// frame.
    ///
    /// # Panics
    ///
    /// Panics if the display update thread stopped, which only happens on a hardware error.
    pub fn swap(&mut self, canvas: LedCanvas) -> LedCanvas {
        let canvas = self
            .matrix
            .update_on_vsync(canvas.canvas)
            .expect("The display update thread stopped");
        LedCanvas { canvas }
    }

    /// The average frame rate over the last 60 frames.
    pub fn framerate(&self) -> usize {
        self.matrix.framerate()
    }
}
