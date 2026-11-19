use rpi_led_panel::{RGBMatrix, RGBMatrixConfig};
use std::str::FromStr;

fn main() {
    // Parse command line arguments for test mode
    let args: Vec<String> = std::env::args().collect();
    let test_mode = args.get(1).map(|s| s.as_str()).unwrap_or("center");

    println!("=== LED Panel Debug Test ===");
    println!("Test mode: {}", test_mode);
    println!("Usage: simple_test [center|offset|edges|noflip]");
    println!();

    let mut config = RGBMatrixConfig {
        rows: 64,
        cols: 128,
        chain_length: 1,
        parallel: 2,
        hardware_mapping: rpi_led_panel::HardwareMapping::regular(),
        slowdown: Some(2),
        ..RGBMatrixConfig::default()
    };

    // Try without Z option for "noflip" mode
    let mapper_str = match test_mode {
        "noflip" => "V-mapper:R", // Without Z option
        _ => "V-mapper:R,Z",
    };

    println!(
        "Config: rows={} cols={} chain={} parallel={}",
        config.rows, config.cols, config.chain_length, config.parallel
    );
    println!("Mapper: {}", mapper_str);

    let mapper = rpi_led_panel::NamedPixelMapperType::from_str(mapper_str)
        .expect("Failed to parse pixel mapper");
    config.pixelmapper.push(mapper);

    let (mut matrix, mut canvas) = RGBMatrix::new(config).expect("Failed to create matrix");

    let width = canvas.width();
    let height = canvas.height();

    println!("Canvas size: {}x{}", width, height);

    // Determine draw positions based on test mode
    let (x_blue, x_red, x_green) = match test_mode {
        "offset" => {
            println!("\n** OFFSET MODE: Drawing at x=62,63,64 (middle of left panel) **");
            println!("If flickering still appears at edges, it's likely hardware timing.");
            (62, 63, 64)
        }
        "edges" => {
            println!("\n** EDGES MODE: Drawing at edges x=0,1 and x=254,255 **");
            (0, 1, 254) // blue at 0, red at 1, green at 254
        }
        "noflip" => {
            println!("\n** NOFLIP MODE: V-mapper:R only (no Z option) **");
            (126, 127, 128)
        }
        _ => {
            println!("\n** CENTER MODE: Drawing at boundary x=126,127,128 **");
            (126, 127, 128)
        }
    };

    println!("Drawing:");
    println!("  - Blue at x={}", x_blue);
    println!("  - Red at x={}", x_red);
    println!("  - Green at x={}", x_green);
    println!("\nWatch for flickering at:");
    println!("  - x=0 (left edge, top half)");
    println!("  - x=255 (right edge, bottom half)");
    println!("\nPress Ctrl+C to exit.\n");

    for i in 0..3600 {
        canvas.fill(0, 0, 0);

        for y in 0..height {
            canvas.set_pixel(x_blue, y, 0, 0, 255);
            canvas.set_pixel(x_red, y, 255, 0, 0);
            canvas.set_pixel(x_green, y, 0, 255, 0);
        }

        canvas = matrix
            .update_on_vsync(canvas)
            .expect("The display update thread stopped");

        if i % 120 == 0 {
            println!("Running: {} seconds (FPS: {})", i / 60, matrix.framerate());
        }
    }

    println!("\nTest complete!");
}
