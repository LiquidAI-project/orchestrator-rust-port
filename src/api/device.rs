//! # device.rs
//!
//! Contains device related items, such as serving device descriptions
//! and healthchecks.

use actix_web::{HttpResponse, Responder, web};
use log::{info, warn, debug, error};
use serde_json::{json, Value};
use sysinfo::{System, Networks};
use serde::Deserialize;
use mongodb::{bson::Bson, bson::to_bson, bson::doc, bson};
use reqwest;
use chrono;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use tokio::time::{sleep, Duration};
use futures::stream::TryStreamExt;
use crate::lib::constants::{
    CONFIG_PATH, 
    DEVICE_HEALTHCHECK_FAILED_THRESHOLD, 
    DEVICE_HEALTH_CHECK_INTERVAL_S,
    COLL_DEVICE
};
use crate::lib::mongodb::{
    find_one, 
    insert_one, 
    update_field,
    get_collection
};
use crate::lib::zeroconf;
use crate::structs::device::{
    CpuInfo, DeviceCommunication, DeviceDescription, DeviceDoc, Health, HealthReport, MemoryInfo, NetworkInterfaceIpInfo, NetworkInterfaceUsage, OsInfo, PlatformInfo, StatusEnum, StatusLogEntry
};
use crate::lib::errors::ApiError;
use crate::lib::utils::default_device_description;

/// Struct used with manual device registrations
#[derive(Debug, Deserialize)]
pub struct ManualDeviceRegistration {
    pub name: Option<String>,
    pub addresses: Option<Vec<String>>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub protocol: Option<String>,
    pub properties: Option<serde_json::Value>,
}


/// GET /health
/// 
/// Returns a system-level health report for the device.
///
/// This endpoint provides diagnostics about:
/// - CPU usage
/// - Memory usage
/// - Per-interface network traffic (bytes up/down)
pub async fn thingi_health() -> Result<impl Responder, ApiError> {
    let mut sys = System::new_all();
    sys.refresh_all();
    let cpu_usage = sys.global_cpu_usage();
    let used = sys.used_memory() as f32;
    let total = sys.total_memory() as f32;
    let memory_usage = if total > 0.0 { (used / total) * 100.0 } else { 0.0 };
    let networks = Networks::new_with_refreshed_list();
    let mut network_usage = std::collections::HashMap::new();
    for (if_name, data) in networks.iter() {
        network_usage.insert(
            if_name.clone(),
            NetworkInterfaceUsage {
                down_bytes: data.total_received(),
                up_bytes: data.total_transmitted(),
            },
        );
    }

    let report = HealthReport {
        cpu_usage,
        memory_usage,
        network_usage,
    };

    debug!("✅ Orchestrator health check done");
    Ok(HttpResponse::Ok().json(report))
}


/// GET /.well-known/wasmiot-device-description
/// 
/// Returns the device description of the orchestrator (generated dynamically)
pub async fn wasmiot_device_description() -> Result<impl Responder, ApiError> {
    debug!("✅ Orchestrator device description served");
    Ok(HttpResponse::Ok().json(get_device_description()))
}


/// GET /.well-known/wot-thing-description
/// 
/// Returns the Web of Things description of the orchestrator (read from instance/config)
pub async fn thingi_description() -> Result<impl Responder, ApiError> {
    debug!("✅ Orchestrator Web of Things description request served");
    Ok(HttpResponse::Ok().json(get_wot_td()))
}


/// Returns dynamic platform info. Since this is the orchestrator,
/// it doesnt provide any supervisor interfaces so that field is left blank.
pub fn get_device_description() -> DeviceDescription {
    DeviceDescription {
        platform: get_device_platform_info(),
        supervisor_interfaces: Vec::new(),
    }
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
pub fn get_device_platform_info() -> PlatformInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let memory_bytes = sys.total_memory();

    let cpu_name = sys.cpus()[0].brand().to_string();
    let clock_speed_hz = sys.cpus()[0].frequency() as u64 * 1_000_000;
    let mut clock_speed: HashMap<String, u64> = HashMap::new();
    clock_speed.insert("Hz".to_string(), clock_speed_hz);
    let core_count = sys.cpus().len();
    
    let system_name = System::name().unwrap_or_default();
    let system_kernel = System::kernel_version().unwrap_or_default();
    let system_os = System::os_version().unwrap_or_default();
    let system_host = System::host_name().unwrap_or_default();

    let networks = Networks::new_with_refreshed_list();
    let mut network_map: HashMap<String, NetworkInterfaceIpInfo> = HashMap::new();
    for (if_name, data) in networks.iter() {
        let ip_info: Vec<String> = data
            .ip_networks()
            .iter()
            .map(|ip| ip.to_string())
            .collect();
        network_map.insert(
            if_name.clone(),
            NetworkInterfaceIpInfo { ip_info },
        );
    }

    PlatformInfo {
        cpu: CpuInfo {
            clock_speed,
            core_count: core_count as u32,
            human_readable_name: cpu_name,
        },
        memory: MemoryInfo { bytes: memory_bytes },
        network: network_map,
        system: OsInfo {
            host_name: system_host,
            kernel: system_kernel,
            name: system_name,
            os: system_os,
        },
    }
}


/// Check whether each discovered device is already in the database.
/// If not, insert it and fetch its description + health asynchronously.
pub async fn process_discovered_devices(devices: Vec<DeviceDoc>) {
    for device in devices {
        // Check if device already exists
        let exists = find_one::<DeviceDoc>(COLL_DEVICE, doc! { "name": &device.name })
            .await
            .unwrap_or(None)
            .is_some();
        if exists {
            continue;
        }

        // If device did not exist, add it into database
        if let Err(e) = insert_one(COLL_DEVICE, &device).await {
            error!("❌ Saving new device failed for '{}': {:?}", device.name, e);
            continue;
        }
        info!("🆕 Found new device '{}'", device.name);

        let device_clone = device.clone();

        // First register the orchestrator to new supervisor. Ignore errors
        // where the registration endpoint is not found, since some supervisors
        // might not have it implemented.
        if let Err(e) = register_orchestrator(&device_clone).await {
            warn!("❗️ Failed to register orchestrator for device '{}': {}", device_clone.name, e);
        } else {
            info!("✅ Registered orchestrator for device '{}'", device_clone.name);
        }

        // For the new device, get the device description and run first health check
        if let Some(desc) = fetch_device_description(&device_clone).await {
            let bson_desc = to_bson(&desc).unwrap_or(Bson::Null);
            let _ = update_field::<DeviceDoc>(COLL_DEVICE, doc! { "name": &device_clone.name }, "description", bson_desc).await;
            info!("📄 '{}' device description fetched", device_clone.name);
        }

        if let Some(report) = fetch_device_health(&device_clone).await {
            let health = Health {
                report,
                time_of_query: chrono::Utc::now(),
            };
            let bson_health = to_bson(&health).unwrap_or(Bson::Null);
            let _ = update_field::<DeviceDoc>(COLL_DEVICE, doc! { "name": &device_clone.name }, "health", bson_health).await;
            info!("📄 '{}' initial healthcheck done ", device_clone.name);
        }
    }
}


/// Attempt to fetch the device description, and parse it into a DeviceDescription.
async fn fetch_device_description(device: &DeviceDoc) -> Option<DeviceDescription> {
    let addr = device.communication.addresses.get(0)?;
    let url = format!(
        "http://{}:{}/.well-known/wasmiot-device-description",
        addr,
        device.communication.port
    );

    match reqwest::get(&url).await {
        Ok(res) if res.status().is_success() => {
            match res.json::<serde_json::Value>().await {
                Ok(v) => {
                    match serde_json::from_value::<DeviceDescription>(v) {
                        Ok(dd) => Some(dd),
                        Err(e) => {
                            warn!("Device '{}' description not in expected shape: {}. Using default.", device.name, e);
                            Some(default_device_description())
                        }
                    }
                }
                Err(e) => {
                    warn!("Device '{}' description JSON error: {}", device.name, e);
                    None
                }
            }
        }
        Ok(res) => {
            warn!("Device '{}' description HTTP status code: {}", device.name, res.status());
            None
        }
        Err(e) => {
            log::warn!("Failed to fetch device description from {}: {}", device.name, e);
            None
        }
    }
}


/// Do a healthcheck on a device.
async fn fetch_device_health(device: &DeviceDoc) -> Option<HealthReport> {
    let h = reqwest::header::HeaderName::from_bytes(b"X-Forwarded-For").unwrap();
    let mut headers = reqwest::header::HeaderMap::new();
    let public_host = std::env::var("PUBLIC_HOST").unwrap_or_else(|_| {
        log::warn!("PUBLIC_HOST environment variable is not set. Using default value 'localhost'");
        "localhost".to_string()
    });
    headers.insert(h, public_host.parse().unwrap());
    let addr = device.communication.addresses.get(0)?;
    let url = format!(
        "http://{}:{}/health",
        addr,
        device.communication.port
    );

    let client = reqwest::Client::new();
    match client.get(&url).headers(headers).send().await {
        Ok(res) if res.status().is_success() => {
            if let Some(header_value) = res.headers().get("Custom-Orchestrator-Set") {
                if let Ok(value) = header_value.to_str() {
                    debug!("Custom-Orchestrator-Set header: {}", value);
                    if value == "false" {
                        info!("Device '{}' requested orchestrator registration", device.name);
                        if let Err(e) = register_orchestrator(device).await {
                            warn!("❗️ Failed to register orchestrator for device '{}': {}", device.name, e);
                        } else {
                            info!("✅ Registered orchestrator for device '{}'", device.name);
                        }
                    }
                }
            }
            match res.json::<serde_json::Value>().await {
                Ok(v) => serde_json::from_value::<HealthReport>(v).ok(),
                Err(e) => {
                    debug!("Invalid health JSON for {}: {}", device.name, e);
                    None
                }
            }
        }
        Ok(res) => {
            debug!("Healthcheck HTTP status code: {}, for device: {}", res.status(), device.name);
            None
        }
        Err(e) => {
            debug!("Failed to do healthcheck for device {}: {}", device.name, e);
            None
        }
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
    let collection = get_collection::<DeviceDoc>(COLL_DEVICE).await;
    let devices: Vec<DeviceDoc> = collection.find(doc! {}).await?
        .try_collect()
        .await?;

    let now = Utc::now();
    let mut ok_count = 0;
    let mut fail_count = 0;
    let mut inactive_count = 0;

    for mut device in devices {

        if device.status == StatusEnum::Inactive {
            inactive_count += 1;
        }

        match fetch_device_health(&device).await {
            Some(report) => {
                device.health = Some(Health {
                    report,
                    time_of_query: now,
                });
                device.failed_health_check_count = 0;
                device.ok_health_check_count += 1;
                ok_count += 1;

                if device.status != StatusEnum::Active && device.ok_health_check_count >= *DEVICE_HEALTHCHECK_FAILED_THRESHOLD {
                    device.status = StatusEnum::Active;
                    let log = device.status_log.get_or_insert(Vec::new());
                    log.insert(0, StatusLogEntry {
                        status: StatusEnum::Active,
                        time: now,
                    });
                    info!("✅ Device '{}' changed to active", device.name);
                }
            }
            None => {
                device.ok_health_check_count = 0;
                device.failed_health_check_count += 1;
                fail_count += 1;
                device.health = None;

                if device.status != StatusEnum::Inactive && device.failed_health_check_count >= *DEVICE_HEALTHCHECK_FAILED_THRESHOLD {
                    device.status = StatusEnum::Inactive;
                    let log = device.status_log.get_or_insert(Vec::new());
                    log.insert(0, StatusLogEntry {
                        status: StatusEnum::Inactive,
                        time: now,
                    });
                    warn!("🔴 Device '{}' changed to inactive", device.name);

                    // TODO: Implement the deployment check logic thing here later
                }
            }
        }

        // Write updates back to mongo
        let update = doc! {
            "$set": {
                "status": bson::to_bson(&device.status)?,
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


/// POST /file/device/discovery/reset
/// 
/// Handler for resetting device discovery
pub async fn reset_device_discovery() -> Result<impl Responder, ApiError> {
    match zeroconf::run_single_mdns_scan(5).await {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => {
            error!("Failed to trigger device rescan: {}", e);
            Err(ApiError::internal_error("Failed to rescan devices"))
        }
    }
}


/// GET /file/device
/// 
/// Returns all known devices from the database.
pub async fn get_all_devices() -> Result<impl Responder, ApiError> {
    let collection = get_collection::<DeviceDoc>(COLL_DEVICE).await;

    match collection.find(doc! {}).await {
        Ok(cursor) => {
            match cursor.try_collect::<Vec<DeviceDoc>>().await {
                Ok(devices) => {
                    let mut v = serde_json::to_value(&devices).map_err(ApiError::internal_error)?;
                    crate::lib::utils::normalize_object_ids(&mut v);
                    Ok(HttpResponse::Ok().json(v))
                },
                Err(e) => {
                    error!("❌ Failed to collect devices: {:?}", e);
                    Err(ApiError::internal_error("Failed to collect devices"))
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to query devices: {:?}", e);
            Err(ApiError::internal_error("Failed to query devices"))
        }
    }
}


/// DELETE /file/device
/// 
/// Deletes all known devices from database
pub async fn delete_all_devices() -> Result<impl Responder, ApiError> {
    match get_collection::<DeviceDoc>(COLL_DEVICE).await
        .delete_many(doc! {})
        .await
    {
        Ok(result) => Ok(HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count }))),
        Err(e) => {
            error!("❌ Failed to delete all devices: {}", e);
            Err(ApiError::internal_error("Failed to delete devices"))
        }
    }
}


/// GET /file/device/{device_id}
/// 
/// Returns a single device by name
pub async fn get_device_by_name(device_name: web::Path<String>) -> Result<impl Responder, ApiError> {
    match find_one::<DeviceDoc>(COLL_DEVICE, doc! { "name": device_name.as_str() }).await {
        Ok(Some(device)) => {
            let mut v = serde_json::to_value(&device).map_err(ApiError::internal_error)?;
            crate::lib::utils::normalize_object_ids(&mut v);
            Ok(HttpResponse::Ok().json(v))
        },
        Ok(None) => Err(ApiError::not_found("Device not found")),
        Err(e) => {
            error!("Failed to retrieve device '{}': {:?}", device_name, e);
            Err(ApiError::internal_error("Failed to retrieve device"))
        }
    }
}


/// DELETE /file/device/{device_id}
/// 
/// Deletes a specific device from database (by its name)
pub async fn delete_device_by_name(path: web::Path<String>) -> Result<impl Responder, ApiError> {
    let name = path.into_inner();

    match get_collection::<DeviceDoc>(COLL_DEVICE).await
        .delete_one(doc! { "name": name.clone() })
        .await
    {
        Ok(result) => {
            if result.deleted_count == 1 {
                Ok(HttpResponse::NoContent().finish())
            } else {
                Err(ApiError::not_found(format!("Device '{}' not found", name)))
            }
        }
        Err(e) => {
            error!("❌ Failed to delete device '{}': {}", name, e);
            Err(ApiError::internal_error("Failed to delete device"))
        }
    }
}


/// POST /file/device/discovery/register
/// 
/// Adds a device to known devices without depending on mdns mechanisms
pub async fn register_device(info: web::Json<ManualDeviceRegistration>) -> Result<impl Responder, ApiError> {
    let name = info.name.clone()
        .or_else(|| info.host.clone())
        .unwrap_or_else(|| "unknown-device".to_string());

    let addresses = info.addresses.clone()
        .or_else(|| info.host.clone().map(|h| vec![h]))
        .unwrap_or_else(|| vec!["127.0.0.1".to_string()]);

    let port = info.port.unwrap_or(5000);

    let device = DeviceDoc {
        id: None,
        name: name.clone(),
        communication: DeviceCommunication { addresses: addresses.clone(), port },
        description: default_device_description(),
        status: StatusEnum::Active,
        ok_health_check_count: 0,
        failed_health_check_count: 0,
        status_log: Some(vec![StatusLogEntry {
            status: StatusEnum::Active,
            time: Utc::now(),
        }]),
        health: None,
    };

    if let Err(e) = insert_one(COLL_DEVICE, &device).await {
        error!("❌ Manual registration failed for '{}': {:?}", device.name, e);
        return Err(ApiError::internal_error("Failed to register device"));
    }

    info!("🆕 Manually registered device '{}'", name);

    // Fetch description and health like mDNS logic
    if let Some(desc) = fetch_device_description(&device).await {
        let bson_desc = to_bson(&desc).unwrap_or(Bson::Null);
        let _ = update_field::<DeviceDoc>(COLL_DEVICE, doc! { "name": &device.name }, "description", bson_desc).await;
        info!("📄 '{}' device description fetched", device.name);
    }

    if let Some(report) = fetch_device_health(&device).await {
        let health = Health {
            report,
            time_of_query: Utc::now(),
        };
        let bson_health = to_bson(&health).unwrap_or(Bson::Null);
        let _ = update_field::<DeviceDoc>(COLL_DEVICE, doc! { "name": &device.name }, "health", bson_health).await;
        info!("📄 '{}' initial healthcheck done", device.name);
    }

    Ok(HttpResponse::NoContent().finish())
}


/// Registers the orchestrator with the supervisor.
/// This is used to inform the supervisor about the orchestrator's URL.
pub async fn register_orchestrator(device: &DeviceDoc) -> Result<(), reqwest::Error> {
    let public_host = std::env::var("PUBLIC_HOST").unwrap_or_else(|_| {
        log::warn!("PUBLIC_HOST environment variable is not set. Using default value 'localhost'");
        "localhost".to_string()
    });
    let public_port = std::env::var("PUBLIC_PORT").unwrap_or_else(|_| {
        log::warn!("PUBLIC_PORT environment variable is not set. Using default value '3000'");
        "3000".to_string()
    });
    let orchestrator_url = format!("http://{}:{}", public_host, public_port);

    let addr = match device.communication.addresses.get(0) {
        Some(a) => a,
        None => {
            info!("Device '{}' has no addresses; skipping registration.", device.name);
            return Ok(());
        }
    };

    debug!("Registering orchestrator to supervisor with following url {:?}", orchestrator_url);
    let url = format!(
        "http://{}:{}/register",
        addr,
        device.communication.port
    );
    if addr == &public_host && device.communication.port.to_string() == public_port {
        info!("Skipping orchestrator self-registration.");
        return Ok(());
    }
    let client = reqwest::Client::new();
    let payload = json!({ "url": orchestrator_url });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        log::info!("Successfully registered orchestrator at {}", url);
        Ok(())
    } else {
        log::warn!(
            "Failed to register orchestrator at {}: status {}",
            url,
            response.status()
        );
        Ok(())
    }
}
