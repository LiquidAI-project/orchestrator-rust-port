//! # device.rs
//!
//! Contains device related items, such as serving device descriptions
//! and healthchecks.

use crate::lib::constants::CONFIG_PATH;
use actix_web::{HttpResponse, Responder};
use log::info;
use serde_json::{json, Value};
use sysinfo::{System, Networks};
use serde::{Serialize, Deserialize};
use chrono;
use std::fs;


/// Represents the device information (supervisor or orchestrator)
/// discovered via mdns. Below is an example of what this would look like
/// as json:
/// 
/// {
///   name: "device-name",
///   communication: {
///     addresses: ["192.168.1.10"],
///     port: 5000
///   },
///   description: {
///     ...
///   },
///   status: "active",
///   ok_health_check_count: 0,
///   failed_health_check_count: 0,
///   status_log: [{ status: "active", time: ... }],
///   health: {
///     report: { ... },
///     time_of_query: ...
///   }
/// }
/// 
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub communication: Communication,
    pub description: Option<serde_json::Value>,
    pub status: String,
    pub ok_health_check_count: u32,
    pub failed_health_check_count: u32,
    pub status_log: Vec<StatusLogEntry>,
    pub health: Option<HealthReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Communication {
    pub addresses: Vec<String>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusLogEntry {
    pub status: String,
    pub time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub report: Option<serde_json::Value>,
    pub time_of_query: chrono::DateTime<chrono::Utc>,
}


/// Returns a system-level health report for the device.
///
/// This endpoint provides diagnostics about:
/// - CPU usage
/// - Memory usage
/// - Per-interface network traffic (bytes up/down)
pub async fn thingi_health() -> impl Responder {
    info!("Health check done");
    let mut sys = System::new_all();
    sys.refresh_all();
    let cpu_usage = sys.global_cpu_usage();
    let memory_usage = sys.used_memory() / sys.total_memory();
    let networks = Networks::new_with_refreshed_list();
    let network_usage: Value = networks.iter()
        .filter_map(|(interface_name, data)| {
            let down_bytes = data.total_received();
            let up_bytes = data.total_transmitted();
            if down_bytes > 0 || up_bytes > 0 {
                Some((
                    interface_name.clone(),
                    json!({
                        "downBytes": down_bytes,
                        "upBytes": up_bytes
                    })
                ))
            } else {
                None
            }
        })
        .collect();
    HttpResponse::Ok().json(json!({
        "cpuUsage": cpu_usage,
        "memoryUsage": memory_usage,
        "networkUsage": network_usage
    }))
}

/// Returns the device description of the orchestrator (generated dynamically)
pub async fn wasmiot_device_description() -> impl Responder {
    info!("Device description request for orchestrator served");
    HttpResponse::Ok().json(get_device_description())
}

/// Returns the Web of Things description of the orchestrator (read from instance/config)
pub async fn thingi_description() -> impl Responder {
    info!("Web of Things description request for orchestrator served");
    HttpResponse::Ok().json(get_wot_td())
}

/// Returns dynamic platform info. Since this is the orchestrator,
/// it doesnt provide any supervisor interfaces so that field is left blank.
pub fn get_device_description() -> Value {
    let mut description: Value = json!({});
    description["platform"] = get_device_platform_info();
    description["supervisorInterfaces"] = json!([]);
    description
}

/// Loads the Web of Things (WoT) Thing Description from `device-description.json`.
/// This is a file expected to exist in the ./instance/config directory.
pub fn get_wot_td() -> Value {
    let path = CONFIG_PATH.join("device-description.json");
    let file_str = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Could not open or read {}", path.display()));
    serde_json::from_str(&file_str)
        .unwrap_or_else(|e| panic!("Error parsing JSON in {}: {}", path.display(), e))
}

/// Gathers live system information using the `sysinfo` crate, including:
/// - System name, kernel, OS version, hostname
/// - CPU brand, clock speed, core count
/// - Total memory
/// - Network interfaces and IP addresses
///
/// This data is used in the WasmIoT device description function.
pub fn get_device_platform_info() -> Value {
    let mut sys = System::new_all();
    sys.refresh_all();

    let memory_bytes = sys.total_memory();
    let cpu_name = sys.cpus()[0].brand().to_string();
    let clock_speed_hz = sys.cpus()[0].frequency() as u64 * 1_000_000;
    let core_count = sys.cpus().len();

    let system_name = System::name();
    let system_kernel = System::kernel_version();
    let system_os = System::os_version();
    let system_host = System::host_name();

    let networks = Networks::new_with_refreshed_list();
    let network_data: Value = networks.iter()
        .map(|(interface_name, data)| {
            (
                interface_name.clone(),
                json!({
                    "ipInfo": data.ip_networks()
                        .iter()
                        .map(|ip| ip.to_string())
                        .collect::<Vec<String>>()
                }),
            )
        })
        .collect();

    json!({
        "system": {
            "name": system_name,
            "kernel": system_kernel,
            "os": system_os,
            "hostName": system_host
        },
        "memory": {
            "bytes": memory_bytes
        },
        "cpu": {
            "humanReadableName": cpu_name,
            "clockSpeed": {
                "Hz": clock_speed_hz
            },
            "coreCount": core_count
        },
        "network": network_data
    })
}
