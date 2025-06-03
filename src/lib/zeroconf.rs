//! # zeroconf.rs
//!
//! This module handles zero-configuration networking for the Wasm orchestrator service.
//!
//! Currently, contains logic only for browsing services and registering them, but not
//! advertising a service. The advertising logic is currently under the supervisor repository.


use log::info;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use zeroconf::prelude::*;
use zeroconf::{MdnsBrowser, ServiceType};

// Mdns browser
// TODO: Another version that can be called via reset device discovery, triggering a single device refresh
// TODO: Add separate device discovery related logic, and move it to device.rs
pub fn browse_services() -> zeroconf::Result<()> {

    std::thread::spawn(move || {
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
                    thread::sleep(Duration::from_secs(60));
                    continue;
                }
            };
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                // Poll for devices for 5 seconds each run
                if let Err(e) = event_loop.poll(Duration::from_millis(250)) {
                    info!("❌ Poll error: {:?}", e);
                    break;
                }
            }
            let result = discovered.lock().unwrap();
            info!("🔁 Discovery complete. Found {} services.", result.len());

            // Sleep until the next 60-second interval
            thread::sleep(Duration::from_secs(60));
        }
    });

    Ok(())
}