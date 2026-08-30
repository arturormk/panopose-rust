use thiserror::Error;

pub type Result<T> = std::result::Result<T, PanoposeError>;

#[derive(Debug, Error)]
pub enum PanoposeError {
    #[error(
        "image dimensions are not a plausible full-sphere equirectangular panorama: {width}x{height}"
    )]
    InvalidEquirectangularDimensions { width: u32, height: u32 },

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
