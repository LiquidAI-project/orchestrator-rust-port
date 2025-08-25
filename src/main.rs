use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde_json::json;
use actix_cors::Cors;
use orchestrator::api::device::{
    wasmiot_device_description, 
    thingi_description,
    thingi_health,
    run_health_check_loop,
    reset_device_discovery,
    get_all_devices,
    get_device_by_name,
    delete_all_devices,
    delete_device_by_name,
    register_device
};
use orchestrator::api::logs::{post_supervisor_log, get_supervisor_logs};
use orchestrator::api::data_source_cards::{
    get_data_source_card, 
    create_data_source_card,
    delete_all_data_source_cards,
    delete_data_source_card_by_nodeid
};
use orchestrator::api::node_cards::{
    create_node_card, 
    get_node_cards, 
    delete_all_node_cards, 
    delete_node_card_by_id
};
use orchestrator::api::zones_and_risk_levels::{
    parse_zones_and_risk_levels, 
    get_zones_and_risk_levels, 
    delete_all_zones_and_risk_levels
};
use orchestrator::api::module::{
    create_module,
    delete_all_modules,
    delete_module_by_id,
    get_all_modules,
    get_module_by_id,
    describe_module,
    get_module_description_by_id,
    get_module_datafile
};
use orchestrator::api::module_cards::{
    create_module_card, 
    get_module_cards,
    delete_all_module_cards, 
    delete_module_card_by_id
};
use orchestrator::lib::zeroconf;
use log::{error, debug};
use actix_web::middleware::NormalizePath;

// Placeholder handler
async fn placeholder(req: HttpRequest) -> impl Responder {
    let match_name = req.match_name().unwrap_or("<no match name>");
    let match_pattern = req.match_pattern().unwrap_or("<no match pattern>".to_string());
    debug!("{}, {}, {}", req.full_url().as_str(), match_name, match_pattern);
    HttpResponse::Ok().json(json!([]))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    println!("\n\nOrchestrator performing initialization tasks..");

    // Load enviroment variables from .env if available
    match dotenv::dotenv() {
        Ok(path) => println!("... Loaded .env from {:?}", path),
        Err(err) => println!("Could not load .env file: {:?}", err),
    }

    // Initialize logging with default level = info (unless overridden by env)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Start mdns browser to start polling for available supervisors
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(zeroconf::browse_services());
    });

    // Start advertising orchestrator to itself via mdns
    let zc = zeroconf::WebthingZeroconf::new();
    if let Err(e) = zeroconf::register_service(zc) {
        error!("Failed to start mDNS advertisement: {}", e);
    } else {
        debug!("Mdns advertisement started succesfully.");
    }

    println!("... Device discovery setup done.");

    // Start a separate loop to perform continous healthchecks on known devices
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(run_health_check_loop());
    });

    println!("... Healthcheck loop started");

    println!("✅ Initialization tasks done, starting server ...\n");

    HttpServer::new(move || {
        App::new()
            // Add cors and a logger
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600)
            )
            .wrap(
                actix_web::middleware::Logger::default()
            )
            .wrap(
                NormalizePath::trim()
            )

            // Add the client so it can be used in every route
            // .app_data(web::Data::new(client.clone()))

            // Basic routes related to device information and health status
            // Status of implementations:
            // ✅ GET /.well-known/wasmiot-device-description
            // ✅ GET /.well-known/wot-thing-description
            // ✅ GET /health
            .service(web::resource("/.well-known/wasmiot-device-description").name("/.well-known/wasmiot-device-description")
                .route(web::get().to(wasmiot_device_description))) // Get device description
            .service(web::resource("/.well-known/wot-thing-description").name("/.well-known/wot-thing-description")
                .route(web::get().to(thingi_description))) // Get device wot description (doesnt appear to be implemented in original)
            .service(web::resource("/health").name("/health")
                .route(web::get().to(thingi_health))) // Get device current health

            // Device related routes (file: routes/device)
            // Status of implementations:
            // ✅ GET /file/device
            // ✅ DELETE /file/device
            // ✅ GET /file/device/{device_id}
            // ✅ DELETE /file/device/{device_id}
            // ✅ POST /file/device/discovery/reset
            // ✅ POST /file/device/discovery/register
            .service(web::resource("/file/device").name("/file/device")
                .route(web::get().to(get_all_devices)) // Get all devices
                .route(web::delete().to(delete_all_devices))) // Delete all devices
            .service(web::resource("/file/device/{device_name}").name("/file/device/{device_name}")
                .route(web::get().to(get_device_by_name)) // Get device info on specific device. (Doesnt exist in original.)
                .route(web::delete().to(delete_device_by_name))) // Delete a specific device. (Doesnt exist in original.)
            .service(web::resource("/file/device/discovery/reset").name("/file/device/discovery/reset")
                .route(web::post().to(reset_device_discovery))) // Forces the start of a new device scan without waiting for the next one (they happen at regular intervals)
            .service(web::resource("/file/device/discovery/register").name("/file/device/discovery/register")
                .route(web::post().to(register_device))) // Supervisors can force device registration through this endpoint

            // Log related routes (file: routes/logs)
            // Status of implementations:
            // ✅ GET /device/logs
            // ✅ POST /device/logs
            .service(web::resource("/device/logs").name("/device/logs")
                .route(web::get().to(get_supervisor_logs)) // Get all supervisor logs from database
                .route(web::post().to(post_supervisor_log))) // Save a supervisor log to database

            // Module related routes (file: routes/modules)
            // Status of implementations:
            // ✅ POST /file/module
            // ✅ GET /file/module
            // ✅ DELETE /file/module
            // ✅ GET /file/module/{module_id}
            // ✅ DELETE /file/module/{module_id}
            // ✅ POST /file/module/{module_id}/upload
            // ✅ GET /file/module/{module_id}/description
            // ✅ GET /file/module/{module_id}/{file_name}
            .service(web::resource("/file/module").name("/file/module")
                .route(web::post().to(create_module)) // Post a new module (requires file upload)
                .route(web::get().to(get_all_modules)) // Get a list of all modules
                .route(web::delete().to(delete_all_modules))) // Delete all modules
            .service(web::resource("/file/module/{module_id}").name("/file/module/{module_id}")
                .route(web::get().to(get_module_by_id)) // Gets a specific module
                .route(web::delete().to(delete_module_by_id))) // Deletes a specific module
            .service(web::resource("/file/module/{module_id}/upload").name("/file/module/{module_id}/upload")
                .route(web::post().to(describe_module))) // Uploads module description for a specific module?
            .service(web::resource("/file/module/{module_id}/description").name("/file/module/{module_id}/description")
                .route(web::get().to(get_module_description_by_id))) // Gets the module description of a specific module
            .service(web::resource("/file/module/{module_id}/{file_name}").name("/file/module/{module_id}/{file_name}")
                .route(web::get().to(get_module_datafile))) // Serves a file related to module based on module id and file extension/name

            // Manifest/deployment related routes (file: routes/deployment)
            // Status of implementations:
            // ❌ GET /file/manifest
            // ❌ POST /file/manifest
            // ❌ DELETE /file/manifest
            // ❌ GET /file/manifest/{deployment_id}
            // ❌ POST /file/manifest/{deployment_id}
            // ❌ PUT /file/manifest/{deployment_id}
            // ❌ DELETE /file/manifest/{deployment_id}
            .service(web::resource("/file/manifest").name("/file/manifest") // TODO: For consistency, choose name to be either deployment or manifest, not both
                .route(web::get().to(placeholder)) // Get a list of all deployments/manifests
                .route(web::post().to(placeholder)) // Create a new deployment/manifest
                .route(web::delete().to(placeholder))) // Delete all deployments/manifests
            .service(web::resource("/file/manifest/{deployment_id}").name("/file/manifest/{deployment_id}")
                .route(web::get().to(placeholder)) // Get a specific deployment/manifest
                .route(web::post().to(placeholder)) // Deploy a specific deployment/manifest (send necessary files etc to supervisor/s)
                .route(web::put().to(placeholder)) // Update a specific deployment/manifest
                .route(web::delete().to(placeholder))) // Delete a specific deployment/manifest (doesn't exist in original version)

            // Execution related routes (file: routes/execution)
            // Status of implementations:
            // ❌ POST /execute/{deployment_id}
            .service(web::resource("/execute/{deployment_id}").name("/execute/{deployment_id}")
                .route(web::post().to(placeholder))) // Execute a specific deployment/manifest (assumes it has been deployed earlier)

            // Data source card related routes (file: routes/dataSourceCards)
            // Status of implementations:
            // ✅ GET /dataSourceCards
            // ✅ POST /dataSourceCards
            // ✅ DELETE /dataSourceCards
            // ✅ DELETE /dataSourceCards/{node_id}
            .service(web::resource("/dataSourceCards").name("/dataSourceCards")
                .route(web::get().to(get_data_source_card)) // Get all data source cards
                .route(web::post().to(create_data_source_card)) // Create a new data source card
                .route(web::delete().to(delete_all_data_source_cards))) // Delete all data source cards (Doesnt exist in original)
            .service(web::resource("/dataSourceCards/{node_id}").name("/dataSourceCards/{node_id}")
                .route(web::delete().to(delete_data_source_card_by_nodeid))) // Delete a specific data source card (Doesnt exist in original)

            // Deployment certificate related routes (file: routes/deploymentCertificates)
            // Status of implementations:
            // ❌ GET /deploymentCertificates
            .service(web::resource("/deploymentCertificates").name("/deploymentCertificates")
                .route(web::get().to(placeholder))) // Get a list of all deployment certificates (created by the orchestrator, not the user)

            // Module card related routes (file: routes/moduleCards)
            // Status of implementations:
            // ✅ GET /moduleCards
            // ✅ POST /moduleCards
            // ✅ DELETE /moduleCards
            // ✅ DELETE /moduleCards/{card_id}
            .service(web::resource("/moduleCards").name("/moduleCards")
                .route(web::get().to(get_module_cards)) // Get all module cards
                .route(web::post().to(create_module_card)) // Create a new module card
                .route(web::delete().to(delete_all_module_cards))) // Delete all module cards (Doesnt exist in original version)
            .service(web::resource("/moduleCards/{card_id}").name("/moduleCards/{card_id}")
                .route(web::delete().to(delete_module_card_by_id))) // Delete a specific module card (Doesnt exist in original version)

            // Node card related routes (file: routes/nodeCards)
            // Status of implementations:
            // ✅ GET /nodeCards
            // ✅ POST /nodeCards
            // ✅ DELETE /nodeCards
            // ✅ DELETE /nodeCards/{card_id}
            .service(web::resource("/nodeCards").name("/nodeCards")
                .route(web::get().to(get_node_cards)) // Get all node cards
                .route(web::post().to(create_node_card)) // Create a new node card
                .route(web::delete().to(delete_all_node_cards))) // Delete all node cards (Doesnt exist in original version)
            .service(web::resource("/nodeCards/{card_id}").name("/nodeCards/{card_id}")
                .route(web::delete().to(delete_node_card_by_id))) // Delete a specific node card (Doesnt exist in original version)

            // Zone and risk level related routes (file: routes/zonesAndRiskLevels)
            // TODO: Should multiple definitions for zones and risk levels be allowed
            // Status of implementations:
            // ✅ GET /zoneRiskLevels
            // ✅ POST /zoneRiskLevels
            // ✅ DELETE /zoneRiskLevels
            .service(web::resource("/zoneRiskLevels").name("/zoneRiskLevels")
                .route(web::get().to(get_zones_and_risk_levels)) // Get zone and risk level card
                .route(web::post().to(parse_zones_and_risk_levels)) // Create a new zone and risk level card
                .route(web::delete().to(delete_all_zones_and_risk_levels))) // Delete all zones and risk levels (Doesnt exist in original version)

            // Miscellaneous routes, none of these exist in original version, but these are possible improvements for functionality
            // Status of implementations:
            // ❌ POST /postResult
            .service(web::resource("/postResult").name("/postResult")
                .route(web::post().to(placeholder))) // For posting intermediary results in a longer chain of functions/modules

            // Serve frontend static files
            .service(actix_files::Files::new("/", "./frontend").index_file("index.html"))
            
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}
