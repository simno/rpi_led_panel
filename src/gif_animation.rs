//! Playing animated GIFs on a [`Canvas`] of any size.
//!
//! [`GifAnimation`] decodes all frames up front, composites them as the GIF format specifies, and scales them
//! to the canvas size, so drawing a frame is only a copy. The scaling averages in linear light, which keeps
//! detail and brightness when shrinking a large GIF onto a small panel.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    fs::File,
    io::{self, BufReader, Read},
    path::Path,
    time::Duration,
};

use gif::{ColorOutput, DecodeOptions, DisposalMethod, Repeat};

use crate::{Canvas, color::cie1931};

/// Browsers show frames with a delay of 0 or 10 ms for 100 ms, and GIFs are made with that in mind.
const MIN_FRAME_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_FRAME_DELAY: Duration = Duration::from_millis(100);

/// How a GIF whose aspect ratio differs from the canvas is fitted onto it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ScaleMode {
    /// Scale to fit inside the canvas, keeping the aspect ratio. The rest of the canvas is black.
    #[default]
    Fit,
    /// Scale to cover the whole canvas, keeping the aspect ratio. The edges that stick out are cut off.
    Fill,
    /// Scale to exactly the canvas size, distorting the aspect ratio if needed.
    Stretch,
    /// Don't scale, but center on the canvas, cutting off what doesn't fit.
    Center,
}

/// Why a GIF could not be loaded.
#[derive(Debug)]
#[non_exhaustive]
pub enum GifError {
    /// The file could not be read.
    Io(io::Error),
    /// The data is not a valid GIF.
    Decoding(gif::DecodingError),
    /// The GIF or the canvas has no pixels, or the GIF has no frames.
    Empty,
}

impl Display for GifError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            GifError::Io(error) => write!(f, "Failed to read the GIF: {error}"),
            GifError::Decoding(error) => write!(f, "Failed to decode the GIF: {error}"),
            GifError::Empty => write!(f, "The GIF or the canvas has no pixels or frames."),
        }
    }
}

impl Error for GifError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GifError::Io(error) => Some(error),
            GifError::Decoding(error) => Some(error),
            GifError::Empty => None,
        }
    }
}

impl From<io::Error> for GifError {
    fn from(error: io::Error) -> Self {
        GifError::Io(error)
    }
}

impl From<gif::DecodingError> for GifError {
    fn from(error: gif::DecodingError) -> Self {
        GifError::Decoding(error)
    }
}

/// One frame of a [`GifAnimation`], already scaled to the canvas size.
#[derive(Clone)]
pub struct GifFrame {
    width: usize,
    height: usize,
    /// Linear light per channel, 0 to `u16::MAX`.
    pixels: Vec<[u16; 3]>,
    delay: Duration,
}

impl fmt::Debug for GifFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("GifFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("delay", &self.delay)
            .finish_non_exhaustive()
    }
}

impl GifFrame {
    /// How long the frame is shown.
    #[must_use]
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Draw the frame onto the canvas, overwriting every pixel. If the canvas has a different size than the
    /// one the animation was loaded for, the frame is drawn in the top left corner.
    pub fn draw(&self, canvas: &mut Canvas) {
        let width = self.width.min(canvas.width());
        let height = self.height.min(canvas.height());
        let scale = 1.0 / f32::from(u16::MAX);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = self.pixels[y * self.width + x].map(|c| f32::from(c) * scale);
                canvas.set_pixel_linear(x, y, r, g, b);
            }
        }
    }
}

/// An animated (or still) GIF, scaled to a canvas size and ready to play.
///
/// ```no_run
/// use rpi_led_panel::{GifAnimation, RGBMatrix, RGBMatrixConfig, ScaleMode};
/// use std::time::Instant;
///
/// let (mut matrix, mut canvas) = RGBMatrix::new(RGBMatrixConfig::default())?;
/// let animation = GifAnimation::open("cat.gif", canvas.width(), canvas.height(), ScaleMode::Fit)?;
/// let start = Instant::now();
/// loop {
///     animation.draw(&mut canvas, start.elapsed());
///     canvas = matrix.update_on_vsync(canvas)?;
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct GifAnimation {
    frames: Vec<GifFrame>,
    /// The sum of all frame delays.
    duration: Duration,
    /// How many times the animation plays, or `None` to loop forever.
    plays: Option<u32>,
    source_size: [usize; 2],
}

impl GifAnimation {
    /// Load a GIF file and scale it to `width` x `height`, usually the size of the canvas.
    pub fn open<P: AsRef<Path>>(
        path: P,
        width: usize,
        height: usize,
        mode: ScaleMode,
    ) -> Result<Self, GifError> {
        Self::from_reader(BufReader::new(File::open(path)?), width, height, mode)
    }

    /// Load a GIF from a reader and scale it to `width` x `height`, usually the size of the canvas.
    pub fn from_reader<R: Read>(
        reader: R,
        width: usize,
        height: usize,
        mode: ScaleMode,
    ) -> Result<Self, GifError> {
        if width == 0 || height == 0 {
            return Err(GifError::Empty);
        }

        let mut options = DecodeOptions::new();
        options.set_color_output(ColorOutput::RGBA);
        let mut decoder = options.read_info(reader)?;
        let source_size = [usize::from(decoder.width()), usize::from(decoder.height())];
        if source_size.contains(&0) {
            return Err(GifError::Empty);
        }

        let scaler = Scaler::new(source_size, [width, height], mode);
        let mut compositor = Compositor::new(source_size);
        let mut frames = Vec::new();
        while let Some(frame) = decoder.read_next_frame()? {
            let screen = compositor.add(frame);
            let delay = match Duration::from_millis(u64::from(frame.delay) * 10) {
                delay if delay < MIN_FRAME_DELAY => DEFAULT_FRAME_DELAY,
                delay => delay,
            };
            frames.push(GifFrame {
                width,
                height,
                pixels: scaler.scale(screen),
                delay,
            });
        }
        if frames.is_empty() {
            return Err(GifError::Empty);
        }

        // The loop count is how often the animation repeats after it played once. Without it, a GIF plays once.
        let plays = match decoder.repeat() {
            Repeat::Infinite => None,
            Repeat::Finite(repeats) => Some(u32::from(repeats) + 1),
        };
        let duration = frames.iter().map(|frame| frame.delay).sum();
        Ok(Self {
            frames,
            duration,
            plays,
            source_size,
        })
    }

    /// Ignore the loop count of the GIF and play it forever.
    #[must_use]
    pub fn looping_forever(mut self) -> Self {
        self.plays = None;
        self
    }

    /// The frames, scaled to the canvas size.
    #[must_use]
    pub fn frames(&self) -> &[GifFrame] {
        &self.frames
    }

    /// How long one play of the animation takes.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// The width and height of the GIF before scaling.
    #[must_use]
    pub fn source_size(&self) -> [usize; 2] {
        self.source_size
    }

    /// Whether the animation has played as often as its loop count says, `elapsed` after it started.
    #[must_use]
    pub fn is_finished(&self, elapsed: Duration) -> bool {
        self.plays
            .is_some_and(|plays| elapsed >= self.duration.saturating_mul(plays))
    }

    /// The frame to show `elapsed` after the animation started. Once it is finished, this is the last frame.
    #[must_use]
    pub fn frame_at(&self, elapsed: Duration) -> &GifFrame {
        let last = self.frames.last().expect("An animation has frames");
        if self.is_finished(elapsed) {
            return last;
        }
        // Durations are at most a few hours, so nanoseconds fit easily.
        let mut position =
            Duration::from_nanos((elapsed.as_nanos() % self.duration.as_nanos()) as u64);
        for frame in &self.frames {
            if position < frame.delay {
                return frame;
            }
            position -= frame.delay;
        }
        last
    }

    /// Draw the frame to show `elapsed` after the animation started onto the canvas.
    pub fn draw(&self, canvas: &mut Canvas, elapsed: Duration) {
        self.frame_at(elapsed).draw(canvas);
    }
}

/// Builds the full images of an animation from GIF frames, which may only cover part of the image and
/// specify what happens to it after they were shown.
struct Compositor {
    width: usize,
    /// RGBA, starting out fully transparent.
    screen: Vec<[u8; 4]>,
    /// The screen to restore after a frame with `DisposalMethod::Previous`.
    previous: Option<Vec<[u8; 4]>>,
    /// The area to clear after the last frame, for `DisposalMethod::Background`.
    clear_area: Option<[usize; 4]>,
}

impl Compositor {
    fn new([width, height]: [usize; 2]) -> Self {
        Self {
            width,
            screen: vec![[0; 4]; width * height],
            previous: None,
            clear_area: None,
        }
    }

    /// Apply the disposal of the last frame, draw the frame and return the resulting image.
    fn add(&mut self, frame: &gif::Frame<'_>) -> &[[u8; 4]] {
        if let Some(previous) = self.previous.take() {
            self.screen = previous;
        }
        if let Some([left, top, right, bottom]) = self.clear_area.take() {
            for y in top..bottom {
                self.screen[y * self.width + left..y * self.width + right].fill([0; 4]);
            }
        }

        let height = self.screen.len() / self.width;
        let left = usize::from(frame.left);
        let top = usize::from(frame.top);
        let frame_width = usize::from(frame.width);
        // Frames may stick out of the logical screen.
        let right = (left + frame_width).min(self.width);
        let bottom = (top + usize::from(frame.height)).min(height);

        match frame.dispose {
            DisposalMethod::Previous => self.previous = Some(self.screen.clone()),
            DisposalMethod::Background => {
                self.clear_area = Some([left.min(right), top.min(bottom), right, bottom]);
            }
            DisposalMethod::Any | DisposalMethod::Keep => {}
        }

        for y in top..bottom {
            for x in left..right {
                let i = ((y - top) * frame_width + (x - left)) * 4;
                let pixel: Option<[u8; 4]> =
                    frame.buffer.get(i..i + 4).and_then(|p| p.try_into().ok());
                // Transparent pixels keep what is underneath.
                if let Some(pixel @ [.., alpha]) = pixel {
                    if alpha != 0 {
                        self.screen[y * self.width + x] = pixel;
                    }
                }
            }
        }
        &self.screen
    }
}

/// The source pixels that make up one target pixel along an axis, with their weights.
type Taps = Vec<(usize, f32)>;

/// Scales images to the target size by averaging the covered source pixels, weighted by how much of them
/// is covered. Shrinking keeps every source pixel's contribution and enlarging repeats pixels, which keeps
/// pixel art sharp at whole-number factors.
struct Scaler {
    source_width: usize,
    /// For every target column, the source columns, or `None` outside of the scaled image.
    columns: Vec<Option<Taps>>,
    rows: Vec<Option<Taps>>,
    /// Source color (0-255) to linear light.
    to_linear: [f32; 256],
}

impl Scaler {
    fn new(source: [usize; 2], target: [usize; 2], mode: ScaleMode) -> Self {
        let [source_width, source_height] = source.map(|s| s as f64);
        let [target_width, target_height] = target.map(|t| t as f64);
        let scale = |factor: f64| {
            [
                (source_width * factor).round().max(1.0),
                (source_height * factor).round().max(1.0),
            ]
        };
        let [scaled_width, scaled_height] = match mode {
            ScaleMode::Fit => {
                scale((target_width / source_width).min(target_height / source_height))
            }
            ScaleMode::Fill => {
                scale((target_width / source_width).max(target_height / source_height))
            }
            ScaleMode::Stretch => [target_width, target_height],
            ScaleMode::Center => [source_width, source_height],
        };

        Self {
            source_width: source[0],
            columns: axis_taps(source[0], scaled_width as usize, target[0]),
            rows: axis_taps(source[1], scaled_height as usize, target[1]),
            to_linear: std::array::from_fn(|c| cie1931(c as f32 * 100.0 / 255.0)),
        }
    }

    fn scale(&self, image: &[[u8; 4]]) -> Vec<[u16; 3]> {
        let mut pixels = Vec::with_capacity(self.columns.len() * self.rows.len());
        for row in &self.rows {
            for column in &self.columns {
                let (Some(row), Some(column)) = (row, column) else {
                    pixels.push([0; 3]);
                    continue;
                };
                let mut sum = [0.0f32; 3];
                for &(y, y_weight) in row {
                    for &(x, x_weight) in column {
                        let [r, g, b, a] = image[y * self.source_width + x];
                        // Transparent pixels show the black background.
                        if a == 0 {
                            continue;
                        }
                        let weight = x_weight * y_weight;
                        for (sum, c) in sum.iter_mut().zip([r, g, b]) {
                            *sum += self.to_linear[c as usize] * weight;
                        }
                    }
                }
                pixels.push(sum.map(|c| (c.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16));
            }
        }
        pixels
    }
}

/// For each of the `target` pixels along an axis, the source pixels it covers when `source` pixels are
/// scaled to `scaled` pixels and centered on the target.
fn axis_taps(source: usize, scaled: usize, target: usize) -> Vec<Option<Taps>> {
    // Negative when the scaled image is larger than the target, so that it is cut off evenly.
    let offset = (target as isize - scaled as isize).div_euclid(2);
    let ratio = source as f64 / scaled as f64;
    (0..target)
        .map(|t| {
            let position = usize::try_from(t as isize - offset)
                .ok()
                .filter(|&p| p < scaled)?;
            let start = position as f64 * ratio;
            let end = (position + 1) as f64 * ratio;
            let taps = (start.floor() as usize..(end.ceil() as usize).min(source))
                .map(|s| {
                    let covered = end.min((s + 1) as f64) - start.max(s as f64);
                    (s, (covered / ratio) as f32)
                })
                .filter(|&(_, weight)| weight > 0.0)
                .collect();
            Some(taps)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gif::{DisposalMethod, Encoder, Frame, Repeat};

    use super::{GifAnimation, ScaleMode, axis_taps};

    const WHITE: [u8; 3] = [255, 255, 255];
    const RED: [u8; 3] = [255, 0, 0];

    /// `(left, top, width, height, pixels, dispose)` of a test frame.
    type TestFrame = (u16, u16, u16, u16, Vec<[u8; 3]>, DisposalMethod);

    /// Encode frames on a `width` x `height` screen.
    fn encode(width: u16, height: u16, repeat: Repeat, frames: &[TestFrame]) -> Vec<u8> {
        let mut data = Vec::new();
        {
            let mut encoder = Encoder::new(&mut data, width, height, &[]).unwrap();
            encoder.set_repeat(repeat).unwrap();
            for (left, top, w, h, pixels, dispose) in frames {
                let rgb: Vec<u8> = pixels.iter().flatten().copied().collect();
                let mut frame = Frame::from_rgb(*w, *h, &rgb);
                frame.left = *left;
                frame.top = *top;
                frame.delay = 5;
                frame.dispose = *dispose;
                encoder.write_frame(&frame).unwrap();
            }
        }
        data
    }

    fn load(data: &[u8], width: usize, height: usize, mode: ScaleMode) -> GifAnimation {
        GifAnimation::from_reader(data, width, height, mode).unwrap()
    }

    #[test]
    fn test_axis_taps() {
        // Shrinking by half averages pairs.
        let taps = axis_taps(4, 2, 2);
        assert_eq!(taps[0], Some(vec![(0, 0.5), (1, 0.5)]));
        assert_eq!(taps[1], Some(vec![(2, 0.5), (3, 0.5)]));
        // Doubling repeats pixels.
        let taps = axis_taps(2, 4, 4);
        assert_eq!(taps[1], Some(vec![(0, 1.0)]));
        assert_eq!(taps[2], Some(vec![(1, 1.0)]));
        // A smaller image is centered.
        let taps = axis_taps(2, 2, 4);
        assert_eq!(taps[0], None);
        assert_eq!(taps[1], Some(vec![(0, 1.0)]));
        assert_eq!(taps[3], None);
        // A larger image is cut off on both sides.
        let taps = axis_taps(4, 4, 2);
        assert_eq!(taps[0], Some(vec![(1, 1.0)]));
        assert_eq!(taps[1], Some(vec![(2, 1.0)]));
    }

    #[test]
    fn test_scale_modes() {
        // A 4x2 white GIF on an 8x8 canvas.
        let data = encode(
            4,
            2,
            Repeat::Infinite,
            &[(0, 0, 4, 2, vec![WHITE; 8], DisposalMethod::Keep)],
        );
        let lit = |mode| {
            let animation = load(&data, 8, 8, mode);
            let pixels = &animation.frames()[0].pixels;
            let rows: Vec<usize> = (0..8).filter(|y| pixels[y * 8 + 3] != [0; 3]).collect();
            let columns: Vec<usize> = (0..8)
                .filter(|x| pixels[2 * 8 + x] != [0; 3] || pixels[4 * 8 + x] != [0; 3])
                .collect();
            (rows, columns)
        };
        // Fit: 8x4 in the middle.
        assert_eq!(lit(ScaleMode::Fit), ((2..6).collect(), (0..8).collect()));
        // Fill and stretch cover everything.
        assert_eq!(lit(ScaleMode::Fill), ((0..8).collect(), (0..8).collect()));
        assert_eq!(
            lit(ScaleMode::Stretch),
            ((0..8).collect(), (0..8).collect())
        );
        // Center: unscaled in the middle.
        assert_eq!(lit(ScaleMode::Center), ((3..5).collect(), (2..6).collect()));

        let animation = load(&data, 8, 8, ScaleMode::Fit);
        assert_eq!(animation.frames()[0].pixels[3 * 8], [u16::MAX; 3]);
    }

    #[test]
    fn test_shrinking_averages_in_linear_light() {
        // Alternating black and white columns shrunk to one pixel are half as bright in linear light.
        let pixels = vec![WHITE, [0, 0, 0], WHITE, [0, 0, 0]];
        let data = encode(
            2,
            2,
            Repeat::Infinite,
            &[(0, 0, 2, 2, pixels, DisposalMethod::Keep)],
        );
        let animation = load(&data, 1, 1, ScaleMode::Stretch);
        let [r, g, b] = animation.frames()[0].pixels[0];
        assert_eq!(r, g);
        assert_eq!(g, b);
        assert!(r.abs_diff(u16::MAX / 2) <= 1, "{r}");
    }

    #[test]
    fn test_disposal() {
        // A red pixel on a white 2x1 screen, moving right.
        let data = encode(
            2,
            1,
            Repeat::Infinite,
            &[
                (0, 0, 2, 1, vec![WHITE, WHITE], DisposalMethod::Keep),
                (0, 0, 1, 1, vec![RED], DisposalMethod::Previous),
                (1, 0, 1, 1, vec![RED], DisposalMethod::Background),
                (0, 0, 1, 1, vec![RED], DisposalMethod::Keep),
            ],
        );
        let animation = load(&data, 2, 1, ScaleMode::Center);
        let red = [u16::MAX, 0, 0];
        let white = [u16::MAX; 3];
        let frames: Vec<_> = animation
            .frames()
            .iter()
            .map(|f| f.pixels.clone())
            .collect();
        assert_eq!(frames[0], [white, white]);
        assert_eq!(frames[1], [red, white]);
        // The previous frame was restored before drawing this one.
        assert_eq!(frames[2], [white, red]);
        // The area of the previous frame was cleared to transparent, which shows black.
        assert_eq!(frames[3], [red, [0; 3]]);
    }

    #[test]
    fn test_timing() {
        let frames: Vec<_> = (0..3)
            .map(|_| (0, 0, 1, 1, vec![WHITE], DisposalMethod::Keep))
            .collect();
        // Played twice: once, then repeated once.
        let data = encode(1, 1, Repeat::Finite(1), &frames);
        let animation = load(&data, 1, 1, ScaleMode::Fit);
        assert_eq!(animation.duration(), Duration::from_millis(150));
        let index = |animation: &GifAnimation, ms| {
            let frame = animation.frame_at(Duration::from_millis(ms));
            animation
                .frames()
                .iter()
                .position(|f| std::ptr::eq(f, frame))
                .unwrap()
        };
        assert_eq!(index(&animation, 0), 0);
        assert_eq!(index(&animation, 49), 0);
        assert_eq!(index(&animation, 50), 1);
        assert_eq!(index(&animation, 160), 0);
        assert!(!animation.is_finished(Duration::from_millis(299)));
        assert!(animation.is_finished(Duration::from_millis(300)));
        assert_eq!(index(&animation, 1000), 2);

        let animation = animation.looping_forever();
        assert!(!animation.is_finished(Duration::from_secs(1000)));
        assert_eq!(index(&animation, 1000), 2);
        assert_eq!(index(&animation, 1050), 0);
    }

    #[test]
    fn test_errors() {
        assert!(GifAnimation::from_reader(&b"not a gif"[..], 8, 8, ScaleMode::Fit).is_err());
        let data = encode(
            1,
            1,
            Repeat::Infinite,
            &[(0, 0, 1, 1, vec![WHITE], DisposalMethod::Keep)],
        );
        assert!(GifAnimation::from_reader(data.as_slice(), 0, 8, ScaleMode::Fit).is_err());
    }
}
