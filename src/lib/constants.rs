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

/// Default directory where modules are stored
pub const MODULE_DIR: &str = "./files/wasm";

/// Directory where execution input files are stored
pub const EXECUTION_INPUT_DIR: &str = "./files/exec";

/// Directory where files given for module execution in advance are stored
/// (Essentially deployment mounts)
pub const MOUNT_DIR: &str = "./files/mounts";

/// Name of the initialization function for Wasm modules
pub const WASMIOT_INIT_FUNCTION_NAME: &str = "_wasmiot_init";

// TODO: Is this kind of filtering necessary?
pub const SUPPORTED_FILE_TYPES: &[&str] = &[
    "application/octet-stream",
    "image/jpeg",
    "image/png",
    // TODO: Something more here?
];

// Get some env vars, preventing the need to read them from env more than once during runtime.
lazy_static! {
    pub static ref INSTANCE_PATH: PathBuf = env::current_dir().unwrap().join("instance");
    pub static ref CONFIG_PATH: PathBuf = env::current_dir().unwrap().join("instance/config");
    pub static ref DEVICE_HEALTH_CHECK_INTERVAL_S: u64 = env::var("DEVICE_HEALTH_CHECK_INTERVAL_S").ok().and_then(|u| u.parse().ok()).unwrap();
    pub static ref DEVICE_HEALTHCHECK_FAILED_THRESHOLD: u32 = env::var("DEVICE_HEALTHCHECK_FAILED_THRESHOLD").ok().and_then(|u| u.parse().ok()).unwrap();
    pub static ref DEVICE_SCAN_DURATION_S: u64 = env::var("DEVICE_SCAN_DURATION_S").ok().and_then(|u| u.parse().ok()).unwrap();
    pub static ref DEVICE_SCAN_INTERVAL_S: u64 = env::var("DEVICE_SCAN_INTERVAL_S").ok().and_then(|u| u.parse().ok()).unwrap();
}
