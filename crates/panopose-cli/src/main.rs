use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use image::ImageReader;
use panopose_core::{
    APP_VERSION, Orientation,
    export::{ExportRequest, export_equirectangular},
    synthetic::generate_validation_panorama,
};

#[derive(Debug, Parser)]
#[command(name = "panopose-cli", author, version = APP_VERSION, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    GenerateValidationPanorama {
        #[arg(long, default_value_t = 4096)]
        width: u32,
        #[arg(long, default_value_t = 2048)]
        height: u32,
        output: PathBuf,
    },
    Export {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long, default_value_t = 180.0)]
        center_azimuth: f64,
        #[arg(long, default_value_t = 0.0)]
        yaw: f64,
        #[arg(long, default_value_t = 0.0)]
        pitch: f64,
        #[arg(long, default_value_t = 0.0)]
        roll: f64,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::GenerateValidationPanorama {
            width,
            height,
            output,
        } => {
            let image = generate_validation_panorama(width, height);
            image
                .save(&output)
                .with_context(|| format!("failed to save {}", output.display()))?;
        }
        Command::Export {
            input,
            output,
            width,
            height,
            center_azimuth,
            yaw,
            pitch,
            roll,
        } => {
            let source = ImageReader::open(&input)
                .with_context(|| format!("failed to open {}", input.display()))?
                .decode()
                .with_context(|| format!("failed to decode {}", input.display()))?;
            let exported = export_equirectangular(
                &source,
                ExportRequest {
                    width,
                    height,
                    center_azimuth_deg: center_azimuth,
                    orientation: Orientation::from_yaw_pitch_roll_deg(yaw, pitch, roll),
                },
            )?;
            exported
                .save(&output)
                .with_context(|| format!("failed to save {}", output.display()))?;
        }
    }

    Ok(())
}
