//! # zeroconf.rs
//!
//! This module handles zero-configuration networking for the Wasm orchestrator service.
//!
//! Contains logic both for advertising the service, as well as browsing other services.
//! Advertising in this case means the orchestrator advertises itself to itself,
//! and browsing means it periodically gets all available supervisors (and itself)
//! to populate the device list.


use log::info;
use local_ip_address;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use std::env;
use serde::Serialize;
use zeroconf::prelude::*;
use zeroconf::{
    MdnsBrowser, 
    ServiceType, 
    MdnsService, 
    TxtRecord
};
use crate::lib::constants::{
    DEFAULT_URL_SCHEME,
    ORCHESTRATOR_DEFAULT_NAME,
    PUBLIC_PORT
};

/// Represents a service that is advertised on the network.
///
/// Includes details such as:
/// - Name and service type (e.g. `_webthing._tcp`)
/// - Host IP and port
/// - Optional service metadata (`properties`) such as TLS info
#[derive(Debug, Serialize, Clone)]
pub struct WebthingZeroconf {
    pub service_name: String,
    pub service_type: String,
    pub service_protocol: String,
    pub host: String,
    pub port: u16,
    pub properties: Vec<(String, String)>,
}

impl WebthingZeroconf {
    /// Constructs a new service representation using env vars or defaults.
    ///
    /// Populates host and port using `get_listening_address()`, reads environment variables
    /// like `PREFERRED_URL_SCHEME` and `ORCHESTRATOR_NAME`, and sets standard `_webthing._tcp`
    /// service type.
    pub fn new() -> Self {
        let (host, port) = get_listening_address();
        let preferred_url_scheme = env::var("PREFERRED_URL_SCHEME")
            .unwrap_or_else(|_| DEFAULT_URL_SCHEME.to_string());
        let tls_flag = if preferred_url_scheme.to_lowercase() == "https" {
            "1"
        } else {
            "0"
        };

        let service_type = "webthing".to_string();
        let service_protocol = "tcp".to_string();
        let service_name = env::var("ORCHESTRATOR_NAME")
            .unwrap_or_else(|_| ORCHESTRATOR_DEFAULT_NAME.to_string());

        let properties = vec![
            ("path".to_string(), "/".to_string()),
            ("tls".to_string(), tls_flag.to_string()),
            ("address".to_string(), host.clone()),
        ];
        WebthingZeroconf {
            service_name,
            service_type,
            service_protocol,
            host,
            port,
            properties,
        }
    }
}

/// Payload structure used when sending service registration info to orchestrator.
#[derive(Debug, Serialize, Clone)]
pub struct ZeroconfRegistrationData<'a> {
    #[serde(rename = "name")]
    name: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    port: u16,
    properties: serde_json::Value,
    addresses: Vec<String>,
    host: String,
}


/// Determines the IP address and port this orchestrator instance is bound to.
/// Defaults to 127.0.0.1 and port 3000
pub fn get_listening_address() -> (String, u16) {
    let host = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
    let port_str = env::var("PUBLIC_PORT")
        .unwrap_or_else(|_| PUBLIC_PORT.to_string());
    let port: u16 = port_str.parse().unwrap_or(PUBLIC_PORT);
    (host, port)
}


// Mdns browser
// TODO: Another version that can be called via reset device discovery, triggering a single device refresh
// TODO: Add separate device discovery related logic, and move it to device.rs
pub fn browse_services() -> zeroconf::Result<()> {

    std::thread::spawn(move || {

        // Read timing configuration from .env
        let scan_duration_secs: u64 = env::var("DEVICE_SCAN_DURATION_S")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        let scan_interval_secs: u64 = env::var("DEVICE_SCAN_INTERVAL_S")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        loop {
            let service_type = ServiceType::new("webthing", "tcp").unwrap();
            let mut browser = MdnsBrowser::new(service_type);
            let discovered = Arc::new(Mutex::new(Vec::new()));
            let discovered_clone = Arc::clone(&discovered);

            browser.set_service_discovered_callback(Box::new(move |result, _| {
                if let Ok(service) = result {
                    info!("🔍 Found: {:?}", service);
                    discovered_clone.lock().unwrap().push(service);
                } else {
                    info!("❌ Discovery error.");
                }
            }));

            let event_loop = match browser.browse_services() {
                Ok(loop_) => loop_,
                Err(e) => {
                    info!("❌ Failed to start browsing: {:?}", e);
                    std::thread::sleep(Duration::from_secs(scan_interval_secs));
                    continue;
                }
            };

            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(scan_duration_secs) {
                if let Err(e) = event_loop.poll(Duration::from_millis(100)) {
                    info!("❌ Poll error: {:?}", e);
                    break;
                }
            }
            let result = discovered.lock().unwrap();
            info!("🔁 Discovery complete. Found {} services.", result.len());

            // Sleep until the next 60-second interval
            thread::sleep(Duration::from_secs(scan_interval_secs));
        }
    });

    Ok(())
}

/// Spawn a separate thread that continuously listens for mdns requests, and
/// responds with orchestrator data when requested.
pub fn register_service(zc: WebthingZeroconf) -> anyhow::Result<()> {
    std::thread::spawn(move || {
        let service_type = ServiceType::new(zc.service_type.as_str(), zc.service_protocol.as_str()).unwrap();
        let mut service = MdnsService::new(service_type, zc.port);
        let mut txt_record = TxtRecord::new();
        zc.properties
            .iter()
            .for_each(|(key, value)| {
                txt_record.insert(key, value).unwrap();
            });
        service.set_name(&zc.service_name);
        service.set_txt_record(txt_record);

        service.set_registered_callback(Box::new(|r, _| {
            if let Ok(svc) = r {
                info!("✅ Responded to mDNS query with: {:?}", svc);
            }
        }));

        let event_loop = service.register().unwrap();
        loop {
            event_loop.poll(Duration::from_secs(1)).unwrap();
        }
    });
    Ok(())
}