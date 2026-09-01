use thiserror::Error;

pub type Result<T> = std::result::Result<T, PanoposeError>;

#[derive(Debug, Error)]
pub enum PanoposeError {
    #[error(
        "image dimensions are not a plausible full-sphere equirectangular panorama: {width}x{height}"
    )]
    InvalidEquirectangularDimensions { width: u32, height: u32 },

    #[error(
        "nadir overlay radius must be greater than 0 and no more than 90 degrees: {radius_deg}"
    )]
    InvalidNadirOverlayRadius { radius_deg: f64 },

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
