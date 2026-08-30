#[cfg(test)]
#[path = "version_logic.rs"]
mod version_logic;

pub const APP_VERSION: &str = env!("PANPOSE_APP_VERSION");
