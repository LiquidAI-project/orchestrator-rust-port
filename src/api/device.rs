//! # device.rs
//!
//! Contains device related items, such as serving device descriptions
//! and healthchecks.

use actix_web::{HttpResponse, Responder};
use log::{info, warn, debug, error};
use serde_json::{json, Value};
use sysinfo::{System, Networks};
use serde::{Serialize, Deserialize};
use mongodb::{bson::Bson, bson::to_bson, bson::doc, bson};
use reqwest;
use chrono;
use chrono::Utc;
use std::fs;
use tokio::time::{sleep, Duration};
use futures::stream::TryStreamExt;
use crate::lib::constants::{
    CONFIG_PATH, 
    DEVICE_HEALTHCHECK_FAILED_THRESHOLD, 
    DEVICE_HEALTH_CHECK_INTERVAL_S
};
use crate::lib::mongodb::{
    find_one, 
    insert_one, 
    update_field,
    get_collection
};
use crate::lib::zeroconf;


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
    debug!("✅ Orchestrator health check done");
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
    debug!("✅ Orchestrator device description served");
    HttpResponse::Ok().json(get_device_description())
}

/// Returns the Web of Things description of the orchestrator (read from instance/config)
pub async fn thingi_description() -> impl Responder {
    debug!("✅ Orchestrator Web of Things description request served");
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


/// Check whether each discovered device is already in the database.
/// If not, insert it and fetch its description + health asynchronously.
pub async fn process_discovered_devices(devices: Vec<DeviceInfo>) {
    for device in devices {
        // Check if device already exists
        let exists = find_one::<DeviceInfo>("device", doc! { "name": &device.name })
            .await
            .unwrap_or(None)
            .is_some();
        if exists {
            continue;
        }

        // If device did not exist, add it into database
        if let Err(e) = insert_one("device", &device).await {
            error!("❌ Saving new device failed for '{}': {:?}", device.name, e);
            continue;
        }
        info!("🆕 Found new device '{}'", device.name);

        let device_clone = device.clone();

        // For the new device, get the device description and run first health check
        if let Some(desc) = fetch_device_description(&device_clone).await {
            let bson_desc = to_bson(&desc).unwrap_or(Bson::Null);
            let _ = update_field::<DeviceInfo>("device", doc! { "name": &device_clone.name }, "description", bson_desc).await;
            info!("📄 '{}' device description fetched", device_clone.name);
        }

        if let Some(health) = fetch_device_health(&device_clone).await {
            let health_report = HealthReport {
                report: Some(health),
                time_of_query: chrono::Utc::now(),
            };
            let bson_health = to_bson(&health_report).unwrap_or(Bson::Null);
            let _ = update_field::<DeviceInfo>("device", doc! { "name": &device_clone.name }, "health", bson_health).await;
            info!("📄 '{}' initial healthcheck done ", device_clone.name);
        }
    }
}


/// Attempt to fetch the device description.
/// Returns parsed JSON on success.
async fn fetch_device_description(device: &DeviceInfo) -> Option<serde_json::Value> {
    let url = format!(
        "http://{}:{}/.well-known/wasmiot-device-description",
        device.communication.addresses[0],
        device.communication.port
    );

    match reqwest::get(&url).await {
        Ok(res) if res.status().is_success() => {
            res.json::<serde_json::Value>().await.ok()
        }
        Err(e) => {
            log::warn!("Failed to fetch device description from {}: {}", device.name, e);
            None
        }
        _ => None,
    }
}


/// Do a healthcheck on a device.
/// Returns parsed JSON on success.
async fn fetch_device_health(device: &DeviceInfo) -> Option<serde_json::Value> {
    let url = format!(
        "http://{}:{}/health",
        device.communication.addresses[0],
        device.communication.port
    );

    match reqwest::get(&url).await {
        Ok(res) if res.status().is_success() => {
            res.json::<serde_json::Value>().await.ok()
        }
        Err(e) => {
            debug!("Failed to do healthcheck for {}: {}", device.name, e);
            None
        }
        _ => None,
    }
}


/// Continous loop for running health checks on known devices
pub async fn run_health_check_loop() {
    loop {  
        if let Err(e) = perform_health_checks().await {
            error!("Health check loop failed: {}", e);
        } else {
            debug!("✅ Device healthchecks completed");
        }
        sleep(Duration::from_secs(*DEVICE_HEALTH_CHECK_INTERVAL_S)).await;
    }
}


/// Performs health checks on all known devices.
/// Will mark devices as inactive if certain number of health checks are failed.
async fn perform_health_checks() -> mongodb::error::Result<()>{
    let collection = get_collection::<DeviceInfo>("device").await;
    let devices: Vec<DeviceInfo> = collection.find(doc! {}).await?
        .try_collect()
        .await?;

    let now = Utc::now();
    let mut ok_count = 0;
    let mut fail_count = 0;
    let mut inactive_count = 0;

    for mut device in devices {
        if device.status == "inactive" {
            inactive_count += 1;
        }
        match fetch_device_health(&device).await {
            Some(report) => {
                device.health = Some(HealthReport {
                    report: Some(report),
                    time_of_query: now,
                });
                device.failed_health_check_count = 0;
                device.ok_health_check_count += 1;
                ok_count += 1;

                if device.status != "active" && device.ok_health_check_count >= *DEVICE_HEALTHCHECK_FAILED_THRESHOLD {
                    device.status = "active".to_string();
                    device.status_log.insert(0, StatusLogEntry {
                        status: "active".into(),
                        time: now,
                    });
                    info!("✅ Device '{}' changed to active", device.name);
                }
            }
            None => {
                device.ok_health_check_count = 0;
                device.failed_health_check_count += 1;
                fail_count += 1;
                device.health = Some(HealthReport {
                    report: None,
                    time_of_query: now,
                });

                if device.status != "inactive" && device.failed_health_check_count >= *DEVICE_HEALTHCHECK_FAILED_THRESHOLD {
                    device.status = "inactive".to_string();
                    device.status_log.insert(0, StatusLogEntry {
                        status: "inactive".into(),
                        time: now,
                    });
                    warn!("🔴 Device '{}' changed to inactive", device.name);

                    // TODO: Implement the deployment check logic thingy here later
                }
            }
        }

        // Write updates back to mongo
        let update = doc! {
            "$set": {
                "status": &device.status,
                "failed_health_check_count": device.failed_health_check_count,
                "ok_health_check_count": device.ok_health_check_count,
                "status_log": bson::to_bson(&device.status_log)?,
                "health": bson::to_bson(&device.health)?,
            }
        };
        collection.update_one(doc! { "name": &device.name }, update).await?;
    }

    info!(
        "\n❤️ Health check summary:\n {} succeeded, {} failed, {} inactive devices",
        ok_count, fail_count, inactive_count
    );

    Ok(())
}

/// Handler for resetting device discovery
pub async fn reset_device_discovery() -> impl Responder {
    match zeroconf::run_single_mdns_scan(5).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            error!("Failed to trigger device rescan: {}", e);
            HttpResponse::InternalServerError().body("Failed to rescan devices")
        }
    }
}

