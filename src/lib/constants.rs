//! # constants.rs
//!
//! This module contains constant values and functions related to constants.

use std::path::PathBuf;
use lazy_static::lazy_static;
use const_format::concatcp;
use std::env;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use sysinfo::{System, Networks, Disks};

/// Default port used when running the service.
pub const PUBLIC_PORT: u16 = 3000;

/// Default URL scheme used in requests etc.
pub const DEFAULT_URL_SCHEME: &str = "http";

/// Default name of the orchestrator
pub const ORCHESTRATOR_DEFAULT_NAME: &str = "orchestrator";

/// Root directory for where files and modules are stored into
pub const FILE_ROOT_DIR: &str = "./files";

/// Default directory where modules are stored
pub const MODULE_DIR: &str = concatcp!(FILE_ROOT_DIR, "/wasm");

/// Directory where execution input files are stored
pub const EXECUTION_INPUT_DIR: &str = concatcp!(FILE_ROOT_DIR, "/exec");

/// Directory where files given for module execution in advance are stored
/// (Essentially deployment mounts)
pub const MOUNT_DIR: &str = concatcp!(FILE_ROOT_DIR, "/mounts");

/// Name of the initialization function for Wasm modules
pub const WASMIOT_INIT_FUNCTION_NAME: &str = "_wasmiot_init";

// Names of collections in MongoDB
pub const COLL_DATASOURCE_CARDS: &str = "datasourcecards";
pub const COLL_DEPLOYMENT: &str = "deployment";
pub const COLL_DEPLOYMENT_CERTS: &str = "deploymentcertificates";
pub const COLL_DEVICE: &str = "device";
pub const COLL_MODULE: &str = "module";
pub const COLL_MODULE_CARDS: &str = "modulecards";
pub const COLL_NODE_CARDS: &str = "nodecards";
pub const COLL_ZONES: &str = "zones";
pub const COLL_LOGS: &str = "supervisorLogs";

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

pub(crate) static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(System::new_all()));
pub(crate) static NETWORKS: Lazy<Mutex<Networks>> = Lazy::new(|| Mutex::new(Networks::new_with_refreshed_list()));
pub(crate) static DISKS: Lazy<Mutex<Disks>> = Lazy::new(|| Mutex::new(Disks::new_with_refreshed_list()));
