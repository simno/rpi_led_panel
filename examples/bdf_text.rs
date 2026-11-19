/// Example demonstrating BDF font text rendering
///
/// This example shows how to load a BDF font and render text on the LED matrix
/// using the draw_text function.
///
/// Usage:
///   cargo run --example bdf_text -- --rows 32 --cols 128 --chain 1
///
use rpi_led_panel::{BdfFont, RGBMatrix, RGBMatrixConfig};
use std::io::Write;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config: RGBMatrixConfig = argh::from_env();
    let cols = config.cols as i32;

    let (mut matrix, mut canvas) = RGBMatrix::new(config).expect("Matrix initialization failed");

    // Load BDF font
    let font = match BdfFont::load("fonts/test_font.bdf") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to load font: {}", e);
            eprintln!("Make sure fonts/test_font.bdf exists");
            return;
        }
    };

    println!(
        "Loaded font '{}' - size: {}, height: {}",
        font.name, font.size, font.height
    );

    let mut frame = 0;
    loop {
        canvas.fill(0, 0, 0);

        // `draw_text` takes the baseline, so this puts the top of the first line at the top of the canvas.
        let y = font.ascent;

        // Example 1: Simple text
        canvas.draw_text(&font, 0, y, 255, 0, 0, "Hello!", 0);

        // Example 2: Another line with different color
        canvas.draw_text(&font, 0, y + font.height as i32, 0, 255, 0, "World!", 0);

        // Example 3: With kerning
        canvas.draw_text(&font, 0, y + 2 * font.height as i32, 0, 0, 255, "Spaced", 1);

        // Show frame counter
        let frame_text = format!("#{}", frame);
        canvas.draw_text(
            &font,
            cols - font.text_width(&frame_text) as i32,
            y + 2 * font.height as i32,
            255,
            255,
            0,
            &frame_text,
            0,
        );

        canvas = matrix
            .update_on_vsync(canvas)
            .expect("The display update thread stopped");

        if frame % 100 == 0 {
            print!("\rFramerate: {}", matrix.framerate());
            std::io::stdout().flush().unwrap();
        }

        frame += 1;
    }
}
