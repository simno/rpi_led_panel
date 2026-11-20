//! Play an animated GIF, scaled to the size of the display.
//!
//! Usage:
//!   cargo run --release --features gif --example gif -- <file.gif> [fit|fill|stretch|center] [matrix options]
//!
//! For example:
//!   cargo run --release --features gif --example gif -- cat.gif fill --rows 32 --cols 64

use std::{io::Write, process::exit, time::Instant};

use argh::FromArgs;
use rpi_led_panel::{GifAnimation, RGBMatrix, RGBMatrixConfig, ScaleMode};

fn main() {
    // Install an env logger since the display will log information and warnings to that.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1).filter(|path| !path.starts_with('-')) else {
        eprintln!(
            "Usage: {} <file.gif> [fit|fill|stretch|center] [matrix options]",
            args[0]
        );
        exit(1);
    };
    let (mode, matrix_args) = match args.get(2).map(String::as_str) {
        Some("fit") => (ScaleMode::Fit, &args[3..]),
        Some("fill") => (ScaleMode::Fill, &args[3..]),
        Some("stretch") => (ScaleMode::Stretch, &args[3..]),
        Some("center") => (ScaleMode::Center, &args[3..]),
        _ => (ScaleMode::Fit, &args[2..]),
    };
    let matrix_args: Vec<&str> = matrix_args.iter().map(String::as_str).collect();
    let config =
        RGBMatrixConfig::from_args(&[&args[0]], &matrix_args).unwrap_or_else(|early_exit| {
            println!("{}", early_exit.output);
            exit(if early_exit.status.is_ok() { 0 } else { 1 });
        });

    let (mut matrix, mut canvas) = RGBMatrix::new(config).expect("Matrix initialization failed");

    let animation = GifAnimation::open(path, canvas.width(), canvas.height(), mode)
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            exit(1);
        })
        .looping_forever();
    let [width, height] = animation.source_size();
    println!(
        "Playing {width}x{height} GIF with {} frames on a {}x{} display, using {} KiB",
        animation.frames().len(),
        canvas.width(),
        canvas.height(),
        animation.memory_size().div_ceil(1024)
    );

    let start = Instant::now();
    for step in 0..=u64::MAX {
        animation.draw(&mut canvas, start.elapsed());
        canvas = matrix
            .update_on_vsync(canvas)
            .expect("The display update thread stopped");

        if step % 120 == 0 {
            print!("\r{:>100}\rFramerate: {}", "", matrix.framerate());
            std::io::stdout().flush().unwrap();
        }
    }
}
