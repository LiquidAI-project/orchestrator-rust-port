//! # constants.rs
//!
//! This module contains constant values and functions related to constants.

use std::path::PathBuf;
use lazy_static::lazy_static;
use std::env;

/// Default port used when running the service.
pub const PUBLIC_PORT: u16 = 3000;

/// Default URL scheme used in requests etc.
pub const DEFAULT_URL_SCHEME: &str = "http";

/// Default name of the orchestrator
pub const ORCHESTRATOR_DEFAULT_NAME: &str = "orchestrator";

// Default path of the instance folder and config folder
lazy_static! {
    pub static ref INSTANCE_PATH: PathBuf = env::current_dir().unwrap().join("instance");
    pub static ref CONFIG_PATH: PathBuf = env::current_dir().unwrap().join("instance/config");
    pub static ref DEVICE_HEALTH_CHECK_INTERVAL_S: u64 = env::var("DEVICE_HEALTH_CHECK_INTERVAL_S").ok().and_then(|u| u.parse().ok()).unwrap();
    pub static ref DEVICE_HEALTHCHECK_FAILED_THRESHOLD: u32 = env::var("DEVICE_HEALTHCHECK_FAILED_THRESHOLD").ok().and_then(|u| u.parse().ok()).unwrap();
}
