// //! This module contains tests that make requests to an orchestrator.
// //! Purpose is to test the supervisor via the orchestrator.
// //! Can also be used to test the orchestrator itself

// use reqwest::Client;
// use serde_json;
// use tokio;
// use log::debug;

// // TODO: Fix/add tests for deploying manifests and executing them
// // TODO: Add tests for creating a module description (or was that handled by orchestrator)
// // TODO: Divide tests into logical sections, like modules/manifests/deployment/execution/device
// // TODO: Make tests more specific
// // TODO: Testing device registration (test_register_device) needs more thought, maybe combine with deleting all devices?
// // TODO: Creating new manifests/deployments requires getting actual modules etc
// // TODO: Combine tests that make sense to combine, like creating a deployment and deleting them
// // TODO: Following tests make orchestrator crash:
// // test_get_core_services
// // test_get_deployment

// #[cfg(test)]
// mod orchestrator_tests {
//     use super::*;


//     // ------------------------- Constants and helper functions -------------------------//


//     const ORCHESTRATOR_URL: &str = "http://localhost:3000";

//     // Set to false to get full stacktrace from tests
//     const SUPPRESS_STACKTRACE: bool = true;

//     /// Helper function to print API test responses
//     async fn print_response(endpoint: &str, response: reqwest::Response) {
//         let status = response.status();
//         let body = response.text().await.unwrap_or_else(|_| "Failed to read body".to_string());

//         debug!("Test: {}", endpoint);
//         debug!("Status Code: {}", status);
//         debug!("Response Body: {}", body);
//     }

    

//     // ------------------------- Device and health related tests ------------------------- //



//     // #[tokio::test]
//     // async fn test_get_core_services() {
//     //     if SUPPRESS_STACKTRACE {
//     //         let f = |_: &std::panic::PanicHookInfo| {};
//     //         std::panic::set_hook(Box::new(f));
//     //     }
//     //     let client = Client::new();
//     //     let response = client.get(format!("{}/core", ORCHESTRATOR_URL))
//     //         .send()
//     //         .await
//     //         .expect("Failed to send request");

//     //     print_response("/core", response).await;
//     // }

//     /// Get device (orchestrator) description
//     #[tokio::test]
//     async fn test_get_device_description() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/.well-known/wasmiot-device-description", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/.well-known/wasmiot-device-description", response).await;
//     }

//     /// Get device (orchestrator) health status
//     #[tokio::test]
//     async fn test_get_health_status() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/health", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/health", response).await;
//     }

//     /// Test getting list of all devices from orchestrator
//     #[tokio::test]
//     async fn test_get_devices() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/file/device", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/file/device", response).await;
//     }

//     /// Test deleting all devices from orchestrator
//     #[tokio::test]
//     async fn test_delete_devices() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.delete(format!("{}/file/device", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/file/device (DELETE)", response).await;
//     }

//     /// Test making the orchestrator rescan for devices
//     #[tokio::test]
//     async fn test_rescan_devices() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.post(format!("{}/file/device/discovery/reset", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/file/device/discovery/reset", response).await;
//     }

//     /// Test device registration
//     // #[tokio::test]
//     // async fn test_register_device() {
//     //     if SUPPRESS_STACKTRACE {
//     //         let f = |_: &std::panic::PanicHookInfo| {};
//     //         std::panic::set_hook(Box::new(f));
//     //     }
//     //     let client = Client::new();
//     //     let request_body = serde_json::json!({
//     //         "addresses": ["192.168.1.10"],
//     //         "host": "test-device",
//     //         "name": "Test Device",
//     //         "port": 5000,
//     //         "protocol": "tcp",
//     //         "properties": { "path": "/", "tls": "0" }
//     //     });

//     //     let response = client.post(format!("{}/devices/discovery/register", ORCHESTRATOR_URL))
//     //         .json(&request_body)
//     //         .send()
//     //         .await
//     //         .expect("Failed to send request");

//     //     print_response("/devices/discovery/register", response).await;
//     // }
    



//     // ------------------------- Module related tests ------------------------- //



//     /// Test getting a list of modules from the orchestrator
//     #[tokio::test]
//     async fn test_get_modules() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/file/module", ORCHESTRATOR_URL))
//             .send().await.expect("Failed to send request");

//         print_response("/file/module", response).await;
//     }

//     /// Test creating a new module to orchestator
//     #[tokio::test]
//     async fn test_create_module() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let request_body = serde_json::json!({
//             "name": "test_module"
//         });

//         let response = client.post(format!("{}/file/module", ORCHESTRATOR_URL))
//             .json(&request_body)
//             .send().await.expect("Failed to send request");

//         print_response("/file/module (POST)", response).await;
//     }

//     /// Test getting a specific module description
//     #[tokio::test]
//     async fn test_get_module_description() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let module_id = "test_module_id";
//         let response = client.get(format!("{}/file/module/{}/description", ORCHESTRATOR_URL, module_id))
//             .send().await.expect("Failed to send request");

//         print_response("/file/module/:id/description", response).await;
//     }



//     // ------------------------- Manifest related tests ------------------------- //



//     /// Test deployment/manifest creation.
//     #[tokio::test]
//     async fn test_post_deploy() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let request_body = serde_json::json!({
//             "status": "todo"
//         });

//         let response = client.post(format!("{}/file/manifest", ORCHESTRATOR_URL))
//             .json(&request_body)
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/file/manifest", response).await;
//     }

//     /// Test getting a list of all deployments/manifests from the orchestrator
//     #[tokio::test]
//     async fn test_get_all_deployments() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/file/manifest", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/file/manifest", response).await;
//     }

//     /// Test getting a specific deployment/manifest from the orchestrator
//     /// NOTE: Might crash the orchestrator if manifest with id doesnt exist?
//     // #[tokio::test]
//     // async fn test_get_deployment() {
//     //     if SUPPRESS_STACKTRACE {
//     //         let f = |_: &std::panic::PanicHookInfo| {};
//     //         std::panic::set_hook(Box::new(f));
//     //     }
//     //     let client = Client::new();
//     //     let deployment_id = "test_id";
//     //     let response = client.get(format!("{}/file/manifest/{}", ORCHESTRATOR_URL, deployment_id))
//     //         .send()
//     //         .await
//     //         .expect("Failed to send request");

//     //     print_response("/file/manifest/:id", response).await;
//     // }

//     /// Test deleting all deployments/manifests
//     #[tokio::test]
//     async fn test_delete_all_deployments() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.delete(format!("{}/file/manifest", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/file/manifest (DELETE)", response).await;
//     }



//     // ------------------------- Deployment related tests ------------------------- //





    

//     // ------------------------- Execution related tests ------------------------- //




//     /// Test executing an existing deployment
//     #[tokio::test]
//     async fn test_execute_deployment() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let deployment_id = "test_deployment_id";
//         let request_body = serde_json::json!({
//             "param1": "value1"
//         });

//         let response = client.post(format!("{}/execution/{}", ORCHESTRATOR_URL, deployment_id))
//             .json(&request_body)
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/execution/:deploymentId", response).await;
//     }

//     /// Test getting the results from an execution
//     #[tokio::test]
//     async fn test_get_execution_result() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let deployment_id = "test_deployment_id";
//         let response = client.get(format!("{}/execution/{}", ORCHESTRATOR_URL, deployment_id))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/execution/:deploymentId", response).await;
//     }



//     // ------------------------- ODRL card related tests -------------------------//



//     /// Tests creating a new datasourcecard to the orchestrator
//     #[tokio::test]
//     async fn test_create_data_source_card() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let request_body = serde_json::json!({
//             "asset": [{ "title": "Test Data Source", "relation": [{"type": "type", "value": "test-type"}] }]
//         });

//         let response = client.post(format!("{}/dataSourceCards", ORCHESTRATOR_URL))
//             .json(&request_body)
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/dataSourceCards", response).await;
//     }

//     /// Tests getting all datasource cards from the orchestrator
//     #[tokio::test]
//     async fn test_get_data_source_cards() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/dataSourceCards", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/dataSourceCards", response).await;
//     }

//     /// Test getting a list of all deployment certificates
//     #[tokio::test]
//     async fn test_get_deployment_certificates() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/deploymentCertificates", ORCHESTRATOR_URL))
//             .send()
//             .await
//             .expect("Failed to send request");

//         print_response("/deploymentCertificates", response).await;
//     }

//     /// Test getting a list of all module cards from orchestrator
//     #[tokio::test]
//     async fn test_get_module_cards() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/moduleCards", ORCHESTRATOR_URL))
//             .send().await.expect("Failed to send request");

//         print_response("/moduleCards", response).await;
//     }

//     /// Test getting all node cards from the orchestrator
//     #[tokio::test]
//     async fn test_get_node_cards() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/nodeCards", ORCHESTRATOR_URL))
//             .send().await.expect("Failed to send request");

//         print_response("/nodeCards", response).await;
//     }

//     /// Test getting all zones and risk levels from orchestrator
//     #[tokio::test]
//     async fn test_get_zones_and_risk_levels() {
//         if SUPPRESS_STACKTRACE {
//             let f = |_: &std::panic::PanicHookInfo| {};
//             std::panic::set_hook(Box::new(f));
//         }
//         let client = Client::new();
//         let response = client.get(format!("{}/zoneRiskLevels", ORCHESTRATOR_URL))
//             .send().await.expect("Failed to send request");

//         print_response("/zoneRiskLevels", response).await;
//     }



//     // ------------------------- Other tests -------------------------//



//     // /// Test getting logs from the orchestrator
//     // #[tokio::test]
//     // async fn test_get_logs() {
//     //     if SUPPRESS_STACKTRACE {
//     //         let f = |_: &std::panic::PanicHookInfo| {};
//     //         std::panic::set_hook(Box::new(f));
//     //     }
//     //     let client = Client::new();
//     //     let response = client.get(format!("{}/logs", ORCHESTRATOR_URL))
//     //         .send().await.expect("Failed to send request");

//     //     print_response("/logs", response).await;
//     // }

//     // /// Test posting a new log to orchestrator
//     // #[tokio::test]
//     // async fn test_post_log() {
//     //     if SUPPRESS_STACKTRACE {
//     //         let f = |_: &std::panic::PanicHookInfo| {};
//     //         std::panic::set_hook(Box::new(f));
//     //     }
//     //     let client = Client::new();
//     //     let log_entry = serde_json::json!({ "logData": "{\"message\": \"Test Log\"}" });

//     //     let response = client.post(format!("{}/logs", ORCHESTRATOR_URL))
//     //         .json(&log_entry)
//     //         .send().await.expect("Failed to send request");

//     //     print_response("/logs (POST)", response).await;
//     // }

// }
