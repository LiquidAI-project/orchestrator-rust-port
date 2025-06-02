use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde_json::json;
use supervisor;
use mongodb::Client;
use orchestrator::lib::mongodb::initialize_client;

// Placeholder handler
async fn placeholder(_client: web::Data<Client>, req: HttpRequest) -> impl Responder {
    let match_name = req.match_name().unwrap_or("<no match name>");
    let match_pattern = req.match_pattern().unwrap_or("<no match pattern>".to_string());
    println!("{}, {}, {}", req.full_url().as_str(), match_name, match_pattern);
    HttpResponse::Ok().json(json!([]))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    println!("Starting orchestrator at http://localhost:3000");

    let client = initialize_client().await.expect("Failed to initialize MongoDB client");

    HttpServer::new(move || {
        App::new()

            // Add the client so it can be used in every route
            .app_data(web::Data::new(client.clone()))

            // "Core" services of the orchestrator (file: routes/coreServices)
            // Status of implementations:
            // ✅ GET /.well-known/wasmiot-device-description
            // ✅ GET /.well-known/wot-thing-description
            // ❌ GET /core
            // ✅ GET /health
            .service(web::resource("/.well-known/wasmiot-device-description").name("/.well-known/wasmiot-device-description")
                .route(web::get().to(supervisor::lib::api::wasmiot_device_description))) // Get device description
            .service(web::resource("/.well-known/wot-thing-description").name("/.well-known/wot-thing-description")
                .route(web::get().to(supervisor::lib::api::thingi_description))) // Get device wot description (doesnt appear to be implemented in original)
            .service(web::resource("/core").name("/core")
                .route(web::get().to(placeholder))) // Get core service list
            .service(web::resource("/health").name("/health")
                .route(web::get().to(supervisor::lib::api::thingi_health))) // Get device current health

            // Device related routes (file: routes/device)
            // Status of implementations:
            // ❌ GET /file/device
            // ❌ DELETE /file/device
            // ❌ GET /file/device/{device_id}
            // ❌ DELETE /file/device/{device_id}
            // ❌ POST /file/device/discovery/reset
            // ❌ POST /file/device/discovery/register
            .service(web::resource("/file/device").name("/file/device")
                .route(web::get().to(placeholder)) // Get all devices
                .route(web::delete().to(placeholder))) // Delete all devices
            .service(web::resource("/file/device/{device_id}").name("/file/device/{device_id}")
                .route(web::get().to(placeholder)) // Get device info on specific device. (Doesnt exist in original.)
                .route(web::delete().to(placeholder))) // Delete a specific device. (Doesnt exist in original.)
            .service(web::resource("/file/device/discovery/reset").name("/file/device/discovery/reset")
                .route(web::post().to(placeholder))) // Forces the start of a new device scan without waiting for the next one (they happen at regular intervals)
            .service(web::resource("/file/device/discovery/register").name("/file/device/discovery/register")
                .route(web::post().to(placeholder))) // Supervisors can force device registration through this endpoint

            // Log related routes (file: routes/logs)
            // Status of implementations:
            // ❌ GET /device/logs
            // ❌ POST /device/logs
            .service(web::resource("/device/logs").name("/device/logs")
                .route(web::get().to(placeholder)) // Get all supervisor logs from database
                .route(web::post().to(placeholder))) // Save a supervisor log to database

            // Module related routes (file: routes/modules)
            // Status of implementations:
            // ❌ POST /file/module
            // ❌ GET /file/module
            // ❌ DELETE /file/module
            // ❌ GET /file/module/{module_id}
            // ❌ DELETE /file/module/{module_id}
            // ❌ POST /file/module/{module_id}/upload
            // ❌ GET /file/module/{module_id}/description
            // ❌ GET /file/module/{module_id}/{file_name}
            .service(web::resource("/file/module").name("/file/module")
                .route(web::post().to(placeholder)) // Post a new module (requires file upload)
                .route(web::get().to(placeholder)) // Get a list of all modules (doesnt explicitly exist in original one)
                .route(web::delete().to(placeholder))) // Delete all modules (doesnt explicitly exist in original one)
            .service(web::resource("/file/module/{module_id}").name("/file/module/{module_id}")
                .route(web::get().to(placeholder)) // Gets a specific module
                .route(web::delete().to(placeholder))) // Deletes a specific module
            .service(web::resource("/file/module/{module_id}/upload").name("/file/module/{module_id}/upload")
                .route(web::post().to(placeholder))) // Uploads module description for a specific module?
            .service(web::resource("/file/module/{module_id}/description").name("/file/module/{module_id}/description")
                .route(web::get().to(placeholder))) // Gets the module description of a specific module
            .service(web::resource("/file/module/{module_id}/{file_name}").name("/file/module/{module_id}/{file_name}")
                .route(web::get().to(placeholder))) // Serves a file related to module based on module id and file extension/name

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
            // ❌ GET /dataSourceCards
            // ❌ POST /dataSourceCards
            // ❌ DELETE /dataSourceCards
            // ❌ GET /dataSourceCards/{card_id}
            // ❌ PUT /dataSourceCards/{card_id}
            // ❌ DELETE /dataSourceCards/{card_id}
            .service(web::resource("/dataSourceCards").name("/dataSourceCards")
                .route(web::get().to(placeholder)) // Get all data source cards
                .route(web::post().to(placeholder)) // Create a new data source card
                .route(web::delete().to(placeholder))) // Delete all data source cards (Doesnt exist in original)
            .service(web::resource("/dataSourceCards/{card_id}").name("/dataSourceCards/{card_id}")
                .route(web::get().to(placeholder)) //Get a specific data source card (Doesnt exist in original)
                .route(web::put().to(placeholder)) // Update a specific data source card (Doesnt exist in original)
                .route(web::delete().to(placeholder))) // Delete a specific data source card (Doesnt exist in original)

            // Deployment certificate related routes (file: routes/deploymentCertificates)
            // Status of implementations:
            // ❌ GET /deploymentCertificates
            .service(web::resource("/deploymentCertificates").name("/deploymentCertificates")
                .route(web::get().to(placeholder))) // Get a list of all deployment certificates (created by the orchestrator, not the user)

            // Module card related routes (file: routes/moduleCards)
            // Status of implementations:
            // ❌ GET /moduleCards
            // ❌ POST /moduleCards
            // ❌ DELETE /moduleCards
            // ❌ GET /moduleCards/{card_id}
            // ❌ PUT /moduleCards/{card_id}
            // ❌ DELETE /moduleCards/{card_id}
            .service(web::resource("/moduleCards").name("/moduleCards")
                .route(web::get().to(placeholder)) // Get all module cards
                .route(web::post().to(placeholder)) // Create a new module card
                .route(web::delete().to(placeholder))) // Delete all module cards (Doesnt exist in original version)
            .service(web::resource("/moduleCards/{card_id}").name("/moduleCards/{card_id}")
                .route(web::get().to(placeholder)) // Get a specific module card (Doesnt exist in original version)
                .route(web::put().to(placeholder)) // Update a specific module card (Doesnt exist in original version)
                .route(web::delete().to(placeholder))) // Delete a specific module card (Doesnt exist in original version)

            // Node card related routes (file: routes/nodeCards)
            // Status of implementations:
            // ❌ GET /nodeCards
            // ❌ POST /nodeCards
            // ❌ DELETE /nodeCards
            // ❌ GET /nodeCards/{card_id}
            // ❌ PUT /nodeCards/{card_id}
            // ❌ DELETE /nodeCards/{card_id}
            .service(web::resource("/nodeCards").name("/nodeCards")
                .route(web::get().to(placeholder)) // Get all node cards
                .route(web::post().to(placeholder)) // Create a new node card
                .route(web::delete().to(placeholder))) // Delete all node cards (Doesnt exist in original version)
            .service(web::resource("/nodeCards/{card_id}").name("/nodeCards/{card_id}")
                .route(web::get().to(placeholder)) // Get a specific node card (Doesnt exist in original version)
                .route(web::put().to(placeholder)) // Update a specific node card (Doesnt exist in original version)
                .route(web::delete().to(placeholder))) // Delete a specific node card (Doesnt exist in original version)

            // Zone and risk level related routes (file: routes/zonesAndRiskLevels)
            // TODO: Should multiple definitions for zones and risk levels be allowed
            // Status of implementations:
            // ❌ GET /zoneRiskLevels
            // ❌ POST /zoneRiskLevels
            // ❌ DELETE /zoneRiskLevels
            .service(web::resource("/zoneRiskLevels").name("/zoneRiskLevels")
                .route(web::get().to(placeholder)) // Get zone and risk level card
                .route(web::post().to(placeholder)) // Create a new zone and risk level card
                .route(web::delete().to(placeholder))) // Delete all zones and risk levels (Doesnt exist in original version)

            // Miscellaneous routes, none of these exist in original version, but these are possible improvements for functionality
            // Status of implementations:
            // ❌ POST /postResult
            .service(web::resource("/postResult").name("/postResult")
                .route(web::post().to(placeholder))) // For posting intermediary results in a longer chain of functions/modules

            // Serve frontend static files
            .service(actix_files::Files::new("/", "./build/frontend").index_file("index.html"))
            
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}
