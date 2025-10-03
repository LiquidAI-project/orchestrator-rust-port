use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::doc;
use serde_json;
use futures::TryStreamExt;
use crate::lib::mongodb::{find_one, get_collection};
use reqwest;
use futures::future::join_all;
use serde_json::Value;
use mongodb::bson;
use serde_json::json;
use actix_web::{
    web::{self, Path},
    HttpResponse, Responder,
};
use log::{warn, debug, error};
use crate::lib::zeroconf::get_listening_address;
use crate::lib::constants::{
    COLL_DEVICE,
    COLL_MODULE,
    COLL_DEPLOYMENT,
    SUPPORTED_FILE_TYPES
};
use crate::structs::device::DeviceDoc;
use crate::structs::module::{
    ModuleDoc,
    MountStage
};
use crate::structs::deployment::{
    DeploymentDoc,
    DeploymentNode,
    Instruction,
    Instructions,
    RequestBody,
    Endpoint,
    OperationRequest,
    OperationResponse,
    DeviceModule,
    DeviceModuleUrls,
    StageMounts,
    MountPathFile,
    MultipartMediaType,
    SchemaObject,
    SchemaProperty,
    SequenceStep
};
use crate::structs::openapi::{
    OpenApiPathItemObject,
    OpenApiOperation,
    ResponseEnum,
    OpenApiSchemaObject,
    OpenApiSchemaEnum,
    RequestBodyEnum,
    OpenApiParameterEnum,
    OpenApiParameterIn,
    OpenApiFormat
};
use crate::api::deployment_certificates::validate_deployment_solution;
use std::time::Duration;
use crate::lib::errors::ApiError;


/// One step in the deployment sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSequenceStep {
    pub device: String, // The _id of the device in mongodb, or "" for any device
    pub module: String, // The _id of the module in mongodb
    pub func: String, // The name of the function to call
}


/// Sequence (and name) sent by the user. The deployment is built based on this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequence {
    // This is the id of an existing deployment. Used when resolving/updating an existing deployment.
    #[serde(rename = "_id", skip_serializing_if="Option::is_none")]
    pub id: Option<String>, 
    pub name: String,
    pub sequence: Vec<ApiSequenceStep>,
}


/// Represents a single step in a sequence where each device and module have
/// been replaced with their corresponding documents from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceItemHydrated {
    pub device: Option<DeviceDoc>,
    pub module: ModuleDoc,
    pub func: String,
}


/// Represents one step in a deployment that has been fully validated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignedStep {
    pub device: DeviceDoc,
    pub module: ModuleDoc,
    pub func: String,
}


/// The result of solving a deployment sequence. Either a new deployment was created (with its id),
/// or an existing deployment was updated (with the full solution).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SolveResult {
    DeploymentId(ObjectId),
    Solution(CreateSolutionResult),
}


/// The full deployment solution that is stored in the deployment document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSolutionResult {
    #[serde(rename = "fullManifest")]
    pub full_manifest: HashMap<String, DeploymentNode>,
    pub sequence: Vec<SequenceStep>,
}


/// GET /file/manifest/{deployment_id}
/// 
/// Endpoint for fetching a specific deployment (by id)
pub async fn get_deployment(
    path: Path<String>,
) -> Result<impl Responder, ApiError> {
    let deployment_id = path.into_inner();
    let coll = get_collection::<DeploymentDoc>(COLL_DEPLOYMENT).await;

    let oid = ObjectId::parse_str(&deployment_id)
        .map_err(|_| ApiError::bad_request(format!("invalid deployment id '{}'", deployment_id)))?;

    match coll.find_one(doc! { "_id": &oid }).await.map_err(ApiError::db)? {
        Some(doc) => {
            let mut v = serde_json::to_value(&doc).map_err(ApiError::internal_error)?;
            crate::lib::utils::normalize_object_ids(&mut v);
            Ok(HttpResponse::Ok().json(v))
        },
        None => Err(ApiError::not_found(format!("no deployment matches id '{}'", deployment_id))),
    }
}


/// GET /file/manifest
/// 
/// Endpoint for fetching ALL deployments
pub async fn get_deployments() -> Result<impl Responder, ApiError> {
    let coll = get_collection::<DeploymentDoc>(COLL_DEPLOYMENT).await;
    let mut cursor = coll.find(doc! {}).await.map_err(ApiError::db)?;
    let mut out: Vec<DeploymentDoc> = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(ApiError::db)? {
        out.push(doc);
    }
    let mut v = serde_json::to_value(&out).map_err(ApiError::internal_error)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}


/// Helper function for checking that the deployment sequence (describing
/// a sequence of device/module/func combinations) has correct format, 
/// specifically that each step has defined a module and a function.
/// Device step can be empty to indicate that the orchestrator should pick
/// the suitable device.
fn validate_sequence(manifest: &Sequence) -> Result<(), String> {
    if manifest.name.is_empty() {
        return Err("manifest must have a name".into());
    }
    if manifest.sequence.is_empty() {
        return Err("manifest must have a sequence of operations".into());
    }
    for (i, node) in manifest.sequence.iter().enumerate() {
        if node.module.is_empty() {
            return Err(format!("manifest node #{i} must have a module"));
        }
        if node.func.trim().is_empty() {
            return Err(format!("manifest node #{i} must have a function"));
        }
    }
    Ok(())
}


/// POST /file/manifest
/// 
/// Endpoint for creating a new deployment.
pub async fn create_deployment(body: web::Json<Sequence>) -> Result<impl Responder, ApiError> {

    // Check that the sequence that was sent has valid format
    if let Err(msg) = validate_sequence(&body) {
        return Err(ApiError::bad_request(msg));
    }

    // Get the url from which modules can be downloaded from (basically orchestrators address)
    let (orchestrator_host, orchestrator_port) = get_listening_address();
    let package_manager_base_url = std::env::var("PACKAGE_MANAGER_BASE_URL")
            .unwrap_or_else(|_| format!("http://{}:{}", orchestrator_host, orchestrator_port));

    // TODO: Is this kind of filtering based on file types even necessary really?
    let supported_file_types = SUPPORTED_FILE_TYPES.to_vec();

    // Create the deployment based on the sequence that was received
    let res = solve(
        &body,
        false,
        &package_manager_base_url,
        &supported_file_types[..],
    ).await
    .map_err(|e| {
        error!("Failed constructing solution for manifest: {e}");
        ApiError::bad_request(e)
    });

    // Return the id of the deployment that was just created in the format the UI expects it, or an error.
    match res {
        Ok(SolveResult::DeploymentId(oid)) => {
            Ok(HttpResponse::Created()
                .content_type("text/plain; charset=utf-8")
                .body(format!("\"{}\"", oid.to_hex())))
        },
        // This shouldnt happen, it would mean the manifest was updated even though resolving was set to false
        Ok(SolveResult::Solution(_)) => {
            let msg = "Failed constructing solution for manifest: manifest was updated instead.";
            error!("{}", msg);
            Err(ApiError::internal_error(msg))
        },
        Err(e) => {
            Err(e)
        }
    }
}


/// POST /file/manifest/{deployment_id}
/// 
/// Endpoint for deploying an existing deployment. This sends the deployment document to the 
/// necessary devices, which then will download the necessary resources (mounts and wasm files) from
/// the orchestrator.
pub async fn http_deploy(path: Path<String>) -> Result<impl Responder, ApiError> {
    let deployment_param = path.into_inner();
    let coll = get_collection::<DeploymentDoc>(COLL_DEPLOYMENT).await;

    // Try getting the deployment by id or name
    let filter = match ObjectId::parse_str(&deployment_param) {
        Ok(oid) => doc! { "_id": oid },
        Err(_) => {
            warn!(
                "Given deployment id '{}' not ObjectId; trying to use it as a name instead",
                deployment_param
            );
            doc! { "name": &deployment_param }
        }
    };

    let Some(deployment) = coll
        .find_one(filter)
        .await
        .map_err(ApiError::db)?
    else {
        return Err(ApiError::not_found(format!(
            "no deployment matches ID or name '{}'",
            deployment_param
        )));
    };

    let dep_id = deployment
        .id
        .as_ref()
        .cloned()
        .ok_or_else(|| ApiError::db("deployment missing _id"))?;

    // Do the actual deployment, and if succesful, mark the deployment as "active" in database
    match deploy(&deployment).await {
        Ok(device_responses) => {
            coll.update_one(
                doc! { "_id": &dep_id },
                doc! { "$set": { "active": true } },
            )
            .await
            .map_err(ApiError::db)?;

            Ok(HttpResponse::Ok().json(json!({ "deviceResponses": device_responses })))
        }
        Err(err) => {
            Err(err)
        }
    }
}


/// DELETE /file/manifest
/// 
/// Endpoint for deleting all deployments.
pub async fn delete_deployments() -> Result<impl Responder, ApiError> {
    let coll = get_collection::<bson::Document>(COLL_DEPLOYMENT).await;
    let res = coll
        .delete_many(doc! {})
        .await
        .map_err(ApiError::db)?;
    Ok(HttpResponse::Ok().json(json!({ "deletedCount": res.deleted_count })))
}


/// DELETE /file/manifest/{deployment_id}
/// 
/// Endpoint for deleting a specific deployment (by its id)
pub async fn delete_deployment(path: Path<String>) -> Result<impl Responder, ApiError> {
    let deployment_id = path.into_inner();
    let oid = ObjectId::parse_str(&deployment_id)
        .map_err(|_| ApiError::bad_request(format!("invalid deployment id '{}'", deployment_id)))?;

    let coll = get_collection::<bson::Document>(COLL_DEPLOYMENT).await;
    let res = coll
        .delete_one(doc! { "_id": oid })
        .await
        .map_err(ApiError::db)?;

    if res.deleted_count == 0 {
        Err(ApiError::not_found(format!("no deployment matches id '{}'", deployment_id)))
    } else {
        Ok(HttpResponse::Ok().json(json!({ "deletedCount": res.deleted_count })))
    }
}


/// PUT /file/manifest/{deployment_id}
/// 
/// Endpoint for updating an existing deployment. Requires that a deployment exists that has
/// a matching id.
pub async fn update_deployment(
    path: Path<String>,
    body: web::Json<Sequence>,
) -> Result<impl Responder, ApiError> {
    let deployment_id = path.into_inner();
    let oid = ObjectId::parse_str(&deployment_id)
        .map_err(|_| ApiError::bad_request(format!("invalid deployment id '{}'", deployment_id)))?;

    let coll = get_collection::<bson::Document>(COLL_DEPLOYMENT).await;

    let Some(old_raw) = coll
        .find_one(doc! { "_id": &oid })
        .await
        .map_err(ApiError::db)?
    else {
        return Err(ApiError::not_found(format!(
            "no deployment matches ID '{}'",
            deployment_id
        )));
    };

    let was_active = old_raw.get_bool("active").unwrap_or(false);
    let old_name = old_raw
        .get_str("name")
        .unwrap_or("")
        .to_string();
    let mut new_manifest = body.into_inner();
    new_manifest.id = Some(oid.to_hex());

    // Get the url from which modules can be downloaded from (basically orchestrators address)
    let (orchestrator_host, orchestrator_port) = get_listening_address();
    let package_manager_base_url = std::env::var("PACKAGE_MANAGER_BASE_URL")
            .unwrap_or_else(|_| format!("http://{}:{}", orchestrator_host, orchestrator_port));

    // TODO: Is this kind of filtering based on file types even necessary really?
    let supported_file_types = SUPPORTED_FILE_TYPES.to_vec();

    let res = solve(
        &new_manifest,
        true,
        &package_manager_base_url,
        &supported_file_types[..],
    )
    .await
    .map_err(|e| {
        error!("Failed updating manifest for deployment: {e}");
        ApiError::internal_error(e)
    })?;

    let solution = match res {
        SolveResult::Solution(s) => s,
        _ => return Err(ApiError::internal_error("unexpected solver result (expected Solution)")),
    };

    // If the deployment was active, re-deploy it on the targeted devices.
    if was_active {

        let updated_deployment_doc = DeploymentDoc {
            id: Some(oid.clone()),
            name: old_name,
            sequence: solution.sequence,
            validation_error: None,
            full_manifest: solution.full_manifest,
            active: Some(true),
        };

        match deploy(&updated_deployment_doc).await {
            Ok(device_responses) => {
                coll.update_one(
                        doc! { "_id": &oid },
                        doc! { "$set": { "active": true } },
                    )
                    .await
                    .map_err(ApiError::db)?;

                Ok(HttpResponse::Ok().json(json!({ "deviceResponses": device_responses })))
            }
            Err(err) => {
                Err(err)
            }
        }
    } else {
        Ok(HttpResponse::NoContent().finish())
    }
}


/// Creates a new deployment or updates an existing one if resolving = true
pub async fn solve(
    deployment_sequence: &Sequence,
    resolving: bool,
    package_manager_base_url: &str,
    supported_file_types: &[&str],
) -> Result<SolveResult, String> {

    debug!("Received a sequence to solve: {:?}", &deployment_sequence);

    // Hydrate the sequence by replacing all device and module ids with their corresponding docs.
    let mut hydrated: Vec<SequenceItemHydrated> = Vec::with_capacity(deployment_sequence.sequence.len());
    for step in &deployment_sequence.sequence {

        // Find the corresponding device doc, if any.
        let device_id = &step.device;
        let device = if device_id.is_empty() {
            None
        } else {
            let device_filter = match ObjectId::parse_str(&step.device) {
                Ok(oid) => doc! { "_id": oid },
                Err(_) => doc! { "name": &step.device },
            };
            let device = find_one::<DeviceDoc>(COLL_DEVICE, device_filter)
                .await
                .map_err(|e| format!("device.findOne error for '{}': {e}", step.device))?
                .ok_or_else(|| format!("device not found by id '{}'", step.device))?;
            Some(device)
        };

        // Find the corresponding module doc, if any
        let module_filter = match ObjectId::parse_str(&step.module) {
            Ok(oid) => doc! { "_id": oid },
            Err(_) => doc! { "name": &step.module },
        };
        let module = find_one::<ModuleDoc>(COLL_MODULE, module_filter)
            .await
            .map_err(|e| format!("module.findOne error for '{}': {e}", step.module))?
            .ok_or_else(|| format!("module not found by id '{}'", step.module))?;

        hydrated.push(SequenceItemHydrated {
            device,
            module,
            func: step.func.clone(),
        });
    }

    // Check the device selection (add devices if they are missing and check requirements)
    let assigned_sequence = check_device_selection(hydrated).await?;

    // Save the assigned sequence, or if resolving (meaning we are updating an existing deployment) get the id of it
    let deployment_id = if resolving {
        let given_id = deployment_sequence
            .id.clone()
            .ok_or_else(|| "resolving=true but deployment_sequence._id is missing".to_string())?;
        let oid = ObjectId::parse_str(given_id).map_err(|e| format!("Deployment id was not valid object id, error: {:?}", e))?;
        oid
    } else {
        let deployment_collection = get_collection::<bson::Document>(COLL_DEPLOYMENT).await;
        let mut doc_to_insert = bson::to_document(deployment_sequence)
            .map_err(|e| format!("serialize manifest failed: {e}"))?;
        doc_to_insert.remove("_id"); // Remove _id to prevent accidentally attempting to overwrite existing deployment
        let res = deployment_collection
            .insert_one(doc_to_insert)
            .await
            .map_err(|e| format!("insert deployment failed: {e}"))?;
        debug!("Inserted deployment, result: {:?}", res);
        res.inserted_id
            .as_object_id()
            .ok_or_else(|| "inserted_id was not an ObjectId".to_string())?
    };

    // Build the actual manifest/deployment
    let solution = create_solution(
        &deployment_id,
        &assigned_sequence,
        package_manager_base_url,
        supported_file_types,
    )?;

    debug!("Created deployment: {:?}", solution);

    // Validate the deployment, but dont stop execution if validation fails
    if let Err(err) = validate_deployment_solution(&deployment_id, &solution).await {
        let dep_coll = get_collection::<bson::Document>(COLL_DEPLOYMENT).await;
        let _ = dep_coll
            .update_one(
                doc! { "_id": &deployment_id },
                doc! { "$set": { "validationError": err.clone() } }
            )
            .await;
    }

    let dep_coll = get_collection::<bson::Document>(COLL_DEPLOYMENT).await;
    let set_doc = bson::to_document(&solution)
        .map_err(|e| format!("serialize solution failed: {e}"))?;
    dep_coll
        .update_one(doc! { "_id": &deployment_id }, doc! { "$set": set_doc })
        .await
        .map_err(|e| format!("update deployment with solution failed: {e}"))?;

    Ok(if resolving {
        SolveResult::Solution(solution)
    } else {
        SolveResult::DeploymentId(deployment_id)
    })
}


/// Helper function that sends the deployment document to given devices.
pub async fn message_device_deploy(device: &DeviceDoc, manifest: &DeploymentNode) -> Result<Value, String> {
    let ip = device
        .communication
        .addresses
        .get(0)
        .map(|s| s.as_str())
        .ok_or_else(|| format!("device '{}' has no ip address", device.name))?;
    let url = format!("http://{}:{}{}", ip, device.communication.port, "/deploy");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("http client build error for device '{}': {e}", device.name))?;

    let mut payload = serde_json::to_value(manifest)
        .map_err(|e| format!("serialize manifest for device '{}': {e}", device.name))?;
    crate::lib::utils::normalize_object_ids(&mut payload);

    let resp = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("request error to device '{}': {e}", device.name))?;

    let status = resp.status();

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read body error from device '{}': {e}", device.name))?;

    if !status.is_success() {
        let body_txt = String::from_utf8_lossy(&bytes).to_string();
        return Err(format!(
            "HTTP {} from device '{}': {}",
            status.as_u16(),
            device.name,
            body_txt
        ));
    }

    Ok(serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string())))
}


/// Send the deployment docs to devices asynchronously
pub async fn deploy(deployment: &DeploymentDoc) -> Result<HashMap<String, Value>, ApiError> {
    let deployment_solution = &deployment.full_manifest;

    let mut tasks = Vec::with_capacity(deployment_solution.len());

    for (device_id_hex, manifest) in deployment_solution.iter() {
        let oid = ObjectId::parse_str(device_id_hex)
            .map_err(|e| ApiError::bad_request(format!("bad device id '{}': {e}", device_id_hex)))?;

        let dev_opt = find_one::<DeviceDoc>(COLL_DEVICE, doc! { "_id": &oid })
            .await
            .map_err(|e| ApiError::db(format!("device.findOne error for '{}': {e}", device_id_hex)))?;

        let device = dev_opt.ok_or_else(|| ApiError::not_found(format!("device not found: {}", device_id_hex)))?;
        let manifest_clone = manifest.clone();
        let device_id_for_map = device_id_hex.clone();

        tasks.push(async move {
            let res = message_device_deploy(&device, &manifest_clone).await;
            (device_id_for_map, res)
        });
    }

    let results = join_all(tasks).await;

    let mut out: HashMap<String, Value> = HashMap::new();
    for (device_id, res) in results {
        match res {
            Ok(val) => {
                out.insert(device_id, val);
            }
            Err(e) => {
                return Err(ApiError::internal_error(format!("deployment failed: {}", e)));
            }
        }
    }

    if out.is_empty() {
        return Err(ApiError::internal_error("deployment failed: empty response"));
    }

    Ok(out)
}


/// Small helper function to generate the path where the functions can be called on the supervisor
pub fn supervisor_execution_path(module_name: &str, func_name: &str) -> String {
    format!("/{{deployment}}/modules/{}/{}", module_name, func_name)
}


/// Helper function that gets a devices id as a string from a device document
fn device_id_hex(d: &DeviceDoc) -> Result<String, String> {
    d.id.as_ref()
        .map(|arg0: &bson::oid::ObjectId| ObjectId::to_hex(*arg0))
        .ok_or_else(|| "device missing _id".into())
}

/// Takes a template of a server url (in form http://{serverIp}:{port}), and uses the given device document
/// to fill out that url
fn fill_server_url(template: &str, dev: &DeviceDoc) -> String {
    let ip = dev
        .communication
        .addresses
        .get(0)
        .map(|s| s.as_str())
        .unwrap_or("localhost");
    template
        .replace("{serverIp}", ip)
        .replace("{port}", &dev.communication.port.to_string())
}


/// Helper function that takes the first operation (if any) defined for a given path/endpoint, and returns it
fn pick_single_operation<'a>(
    item: &'a OpenApiPathItemObject,
) -> Result<(&'static str, &'a OpenApiOperation), String> {
    let mut ops: Vec<(&'static str, &OpenApiOperation)> = Vec::new();
    if let Some(op) = &item.get { ops.push(("get", op)); }
    if let Some(op) = &item.put { ops.push(("put", op)); }
    if let Some(op) = &item.post { ops.push(("post", op)); }
    if let Some(op) = &item.delete { ops.push(("delete", op)); }
    if let Some(op) = &item.options { ops.push(("options", op)); }
    if let Some(op) = &item.head { ops.push(("head", op)); }
    if let Some(op) = &item.patch { ops.push(("patch", op)); }
    if let Some(op) = &item.trace { ops.push(("trace", op)); }

    if ops.len() < 1 {
        return Err(format!("Expected at least one operation on endpoint, found none"));
    }
    // TODO: Currently orchestrator doesnt know what to do if an endpoint has more than one operation (get/post) defined
    // even if the schema allows it. Update this part if in the future orchestrator has need of this.
    if ops.len() > 1 {
        warn!("Endpoint had more than one operation ({:?} total), defaulting to use first one", ops.len());
    }
    Ok(ops[0])
}


/// Helper function that builds everything that goes under the "fullManifest" key in a deployment document
pub fn create_solution(
    deployment_id: &ObjectId,
    sequence: &[AssignedStep],
    package_base_url: &str,
    supported_file_types: &[&str],
) -> Result<CreateSolutionResult, String> {
    let mut deployments_to_devices: HashMap<String, DeploymentNode> = HashMap::new();

    for step in sequence {
        let device_id_str = device_id_hex(&step.device)?;

        debug!("Creating solution, working on device: {:?}", device_id_str);

        // Ensure a deployment node exists for this device. Devices are keyed by their ids.
        let node = deployments_to_devices
            .entry(device_id_str.clone())
            .or_insert_with(|| DeploymentNode {
                deployment_id: deployment_id.clone(),
                modules: Vec::new(),
                endpoints: HashMap::new(),
                instructions: Instructions { modules: HashMap::new() },
                mounts: HashMap::new(),
            });

        // Add module metadata needed by the device (urls from where to retrieve necessary files)
        let module_data_for_device = module_data(&step.module, package_base_url)?;
        node.modules.push(module_data_for_device.clone());

        debug!("Generated module data for device:\n{:?}", module_data_for_device);

        // Find the openapi description of the supervisor execution path.
        // The execution path is the path on the supervisor that you can call to execute a specific function
        let func_path_key = supervisor_execution_path(&step.module.name, &step.func);
        let description_doc = step
            .module
            .description
            .as_ref()
            .ok_or_else(|| format!("module.description is missing for '{}'", step.module.name))?;
        let path_item = description_doc
            .paths
            .get(&func_path_key)
            .ok_or_else(|| {
                format!(
                    "Endpoint path '{}' not found in module '{}'",
                    func_path_key, step.module.name
                )
            })?;

        // Pick a single method (get/post etc) that has been defined for the current endpoint/path 
        let (method_str, op) = pick_single_operation(path_item)?;

        // Look for the "200" response. If it is not defined, return an error.
        // TODO: If other responses need to be implemented, this part needs to change
        let resp_200 = op
            .responses
            .get("200")
            .ok_or_else(|| "Response '200' not defined".to_string())?;

        // Gather information for the "response" section under the "endpoint" section
        let (response_media_type, response_media) = match resp_200 {
            ResponseEnum::OpenApiResponseObject(obj) => {
                let content = obj.content.as_ref()
                    .ok_or_else(|| "response 200 has no content".to_string())?;
                // TODO: The content might have multiple entries, this would ignore them. They dont have that at the moment, but 
                // if those are added some day this part needs to change.
                let (media_type, media) = content.iter()
                    .next()
                    .ok_or_else(|| "response 200 content is empty".to_string())?;

                // Convert Option<OpenApiSchemaEnum> -> Option<OpenApiSchemaObject>
                let schema_obj = match &media.schema {
                    Some(OpenApiSchemaEnum::OpenApiSchemaObject(s)) => Some(s.clone()),
                    Some(OpenApiSchemaEnum::OpenApiReferenceObject(r)) => {
                        return Err(format!("response 200 schema is a $ref ({}), resolver not implemented", r.r#ref));
                    }
                    None => None,
                };
                (media_type.clone(), schema_obj)
            }
            ResponseEnum::OpenApiReferenceObject(obj) => {
                return Err(format!("response 200 is a $ref ({}), resolver not implemented yet", obj.r#ref));
            }
        };

        // Get request body items if they happen to be present
        let request_body_built: Option<RequestBody> = match &op.request_body {
            None => None,
            Some(RequestBodyEnum::OpenApiReferenceObject(r)) => {
                return Err(format!(
                    "requestBody is a $ref ({}), resolver not implemented yet",
                    r.r#ref
                ));
            }
            Some(RequestBodyEnum::OpenApiRequestBodyObject(rb)) => {
                // TODO: Chooses the first entry. In future, if multiple are expected, change this.
                if let Some((mt, media)) = rb.content.iter().next() {
                    let schema_obj = match &media.schema {
                        None => None,
                        Some(OpenApiSchemaEnum::OpenApiSchemaObject(s)) => Some(s.clone()),
                        Some(OpenApiSchemaEnum::OpenApiReferenceObject(r)) => {
                            return Err(format!(
                                "requestBody schema is a $ref ({}), resolver not implemented yet",
                                r.r#ref
                            ));
                        }
                    };
                    Some(RequestBody {
                        media_type: mt.clone(),
                        schema: schema_obj,
                        encoding: media.encoding.clone(),
                    })
                } else {
                    None
                }
            }
        };

        // Get the url of the first server object.
        // TODO: If at some point orchestrator wants to do something like support several execution paths on a supervisor etc, this
        // part will have to change.
        let server_url_template = step
            .module
            .description
            .as_ref()
            .and_then(|desc| desc.servers.as_ref())
            .and_then(|v| v.get(0))
            .ok_or_else(|| "module.servers is missing or empty".to_string())?
            .url
            .clone();
        let url = fill_server_url(&server_url_template, &step.device);
        let path = supervisor_execution_path(&step.module.name, &step.func)
            .replace("{deployment}", &deployment_id.to_hex());

        // Clear out the enum things from some openapi structs.
        let mut parameter_list = Vec::new();
        if let Some(params) = &op.parameters {
            for p in params {
                match p {
                    OpenApiParameterEnum::OpenApiParameterObject(po) => parameter_list.push(po.clone()),
                    OpenApiParameterEnum::OpenApiReferenceObject(r) => {
                        return Err(format!(
                            "parameter is a $ref ({}), resolver not implemented yet",
                            r.r#ref
                        ));
                    }
                }
            }
        }

        // Build the endpoint from all information gathered so far
        let endpoint = Endpoint {
            url,
            path,
            method: method_str.to_string(),
            request: OperationRequest {
                parameters: parameter_list.clone(),
                request_body: request_body_built,
            },
            response: OperationResponse {
                media_type: response_media_type.clone(),
                schema: response_media,
            },
        };

        debug!("Endpoint constructed:\n{:?}", endpoint);

        let stage_mounts = mounts_for(&step.module, &step.func, &endpoint, supported_file_types)?;
        node.endpoints
            .entry(step.module.name.clone())
            .or_default()
            .insert(step.func.clone(), endpoint.clone());

        node.mounts
            .entry(step.module.name.clone())
            .or_default()
            .insert(step.func.clone(), stage_mounts);
    }

    if let Some((dev_id, _node)) = deployments_to_devices
        .iter()
        .find(|(_, n)| n.endpoints.is_empty())
    {
        return Err(format!("no endpoints defined for device '{}'", dev_id));
    }

    for i in 0..sequence.len() {
        let curr = &sequence[i];
        let device_id_str = device_id_hex(&curr.device)?;
        let module_name = &curr.module.name;
        let func_name = &curr.func;

        let source_endpoint = deployments_to_devices
            .get(&device_id_str)
            .and_then(|n| n.endpoints.get(module_name))
            .and_then(|m| m.get(func_name))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "source endpoint missing for device {}, module {}, func {}",
                    device_id_str, module_name, func_name
                )
            })?;

        let forward_endpoint = if i + 1 < sequence.len() {
            let next = &sequence[i + 1];
            let fwd_dev_id = device_id_hex(&next.device)?;
            deployments_to_devices
                .get(&fwd_dev_id)
                .and_then(|n| n.endpoints.get(&next.module.name))
                .and_then(|m| m.get(&next.func))
                .cloned()
        } else {
            None
        };

        let node = deployments_to_devices
            .get_mut(&device_id_str)
            .expect("device node must exist when building instructions");

        node.instructions
            .modules
            .entry(module_name.clone())
            .or_default()
            .insert(
                func_name.clone(),
                Instruction {
                    from: source_endpoint,
                    to: forward_endpoint,
                },
            );
    }

    let mut sequence_as_ids: Vec<SequenceStep> = Vec::with_capacity(sequence.len());
    for (idx, s) in sequence.iter().enumerate() {
        let dev_id: ObjectId = s
            .device
            .id
            .as_ref()
            .cloned()
            .ok_or_else(|| format!("sequence[{idx}] missing device ObjectId"))?;

        let mod_id: ObjectId = s
            .module
            .id
            .as_ref()
            .cloned()
            .ok_or_else(|| format!("sequence[{idx}] missing module ObjectId"))?;

        sequence_as_ids.push(SequenceStep {
            device: dev_id,
            module: mod_id,
            func: s.func.clone(),
        });
    }

    Ok(CreateSolutionResult {
        full_manifest: deployments_to_devices,
        sequence: sequence_as_ids,
    })
}


/// Helper function to convert openapi schema object into a schemaobject.
fn openapi_object_to_simple_schema(
    root: &OpenApiSchemaObject,
) -> Result<SchemaObject, String> {
    match root.r#type.as_deref() {
        Some("object") => {}
        other => return Err(format!("Only object schemas supported for multipart; got {:?}", other)),
    }

    let props = root
        .properties
        .as_ref()
        .ok_or_else(|| "multipart schema has no properties".to_string())?;

    let mut out_props: HashMap<String, SchemaProperty> = HashMap::new();

    for (name, schema_enum) in props {
        match schema_enum {
            OpenApiSchemaEnum::OpenApiSchemaObject(obj) => {
                let ty = obj.r#type.clone().unwrap_or_default();
                let fmt: Option<String> = match obj.format {
                    Some(OpenApiFormat::Binary) => Some("binary".to_string()),
                    _ => None,
                };
                out_props.insert(
                    name.clone(),
                    SchemaProperty {
                        r#type: ty,
                        format: fmt,
                    },
                );
            }
            OpenApiSchemaEnum::OpenApiReferenceObject(r) => {
                return Err(format!(
                    "multipart property '{}' is a $ref ({}), resolver not implemented",
                    name, r.r#ref
                ));
            }
        }
    }

    Ok(SchemaObject {
        r#type: "object".to_string(),
        properties: out_props,
    })
}


/// Converts a request body that is expected to be multipart/form-data into a MultipartMediaType struct
fn request_body_to_multipart(rb: &crate::structs::deployment::RequestBody)
    -> Result<MultipartMediaType, String>
{
    if rb.media_type != "multipart/form-data" {
        return Err(format!("Expected multipart/form-data, got '{}'", rb.media_type));
    }

    let schema = rb
        .schema
        .as_ref()
        .ok_or_else(|| "multipart requestBody missing schema".to_string())?;

    let simple = openapi_object_to_simple_schema(schema)?;

    let encoding = rb
        .encoding
        .as_ref()
        .ok_or_else(|| "multipart requestBody missing encoding".to_string())?
        .clone();

    Ok(MultipartMediaType {
        media_type: rb.media_type.clone(),
        schema: simple,
        encoding,
    })
}


/// Builds the per-stage (deployment/execution/output) mount list for a 
/// given module function on a given endpoint.
pub fn mounts_for(
    module: &ModuleDoc,
    func: &str,
    endpoint: &Endpoint,
    supported_file_types: &[&str],
) -> Result<StageMounts, String> {
    let request = &endpoint.request;
    let response = &endpoint.response;

    let mut request_body_paths: Vec<MountPathFile> = Vec::new();
    if let Some(rb) = &request.request_body {
        if rb.media_type == "multipart/form-data" {
            let mp = request_body_to_multipart(rb)?;
            request_body_paths = MountPathFile::list_from_multipart(&mp)?;

            let func_mounts = module
                .mounts
                .as_ref()
                .ok_or_else(|| format!("mounts missing for module '{}'", module.name))?
                .get(func)
                .ok_or_else(|| format!("mounts missing for module '{}' function '{}'", module.name, func))?;

            for m in request_body_paths.iter_mut() {
                let meta = func_mounts.get(&m.path).ok_or_else(|| {
                    format!(
                        "mount metadata for path '{}' missing for module '{}' function '{}'",
                        m.path, module.name, func
                    )
                })?;
                m.stage = Some(meta.stage.clone());
            }
        }
    }

    let unsupported: Vec<String> = request_body_paths
        .iter()
        .filter(|x| !supported_file_types.iter().any(|mt| mt == &x.media_type))
        .map(|x| x.media_type.clone())
        .collect();
    if !unsupported.is_empty() {
        return Err(format!("Input file types not supported: {:?}", unsupported));
    }

    let mut param_files: Vec<MountPathFile> = request
        .parameters
        .iter()
        .filter(|p| p.r#in == OpenApiParameterIn::RequestBody && !p.name.is_empty())
        .map(|p| MountPathFile {
            path: p.name.clone(),
            media_type: "application/octet-stream".to_string(),
            stage: Some(MountStage::Execution),
        })
        .collect();

    let mut response_files: Vec<MountPathFile> = Vec::new();
    if response.media_type == "multipart/form-data" {
        return Err("multipart/form-data responses require encoding; OperationResponse has no encoding".into());
    } else if supported_file_types.iter().any(|mt| *mt == response.media_type) {
        let func_mounts = module
            .mounts
            .as_ref()
            .ok_or_else(|| format!("mounts missing for module '{}'", module.name))?
            .get(func)
            .ok_or_else(|| format!("mounts missing for module '{}' function '{}'", module.name, func))?;

        let (path, _meta) = func_mounts
            .iter()
            .find(|(_, m)| m.stage == MountStage::Output)
            .ok_or_else(|| {
                format!(
                    "output mount of '{}' expected but is missing for module '{}' function '{}'",
                    response.media_type, module.name, func
                )
            })?;

        response_files.push(MountPathFile {
            path: path.clone(),
            media_type: response.media_type.clone(),
            stage: Some(MountStage::Output),
        });
    }

    let mut execution: Vec<MountPathFile> = Vec::new();
    let mut deployment: Vec<MountPathFile> = Vec::new();
    let mut output: Vec<MountPathFile> = Vec::new();

    let mut all: Vec<MountPathFile> = Vec::new();
    all.append(&mut param_files);
    all.append(&mut request_body_paths);
    all.append(&mut response_files);

    for m in all {
        match m.stage {
            Some(MountStage::Execution) => execution.push(m),
            Some(MountStage::Deployment) => deployment.push(m),
            Some(MountStage::Output) => output.push(m),
            None => return Err("a mount has no stage assigned".into()),
        }
    }

    Ok(StageMounts {
        execution,
        deployment,
        output,
    })
}


/// Helper function that checks if a given device provides all the required 
/// supervisor interfaces for a given module.
fn device_satisfies_module(d: &DeviceDoc, m: &ModuleDoc) -> bool {
    m.requirements.iter().all(|r|
        d.description
            .supervisor_interfaces
            .iter()
            .any(|supervisor_interface| supervisor_interface == &r.name)
    )
}


/// Helper function that checks that a device has been selected for
/// each step in the sequence of a deployment. Selects if hasnt been already.
/// Also checks that the selected device has all the necessary supervisor interfaces
/// that the module needs.
pub async fn check_device_selection(sequence: Vec<SequenceItemHydrated>) -> Result<Vec<AssignedStep>, String> {
    
    // First fetch all devices, and remove orchestrator from the selection since its not capable of running wasm modules.
    // TODO: Better way to identify and remove orchestrator, name is not just "orchestrator" always.
    let device_collection = get_collection::<DeviceDoc>(COLL_DEVICE).await;
    let mut cursor = device_collection.find(doc! {}).await.map_err(|e| format!("Database error when trying to get all devices. Error: {:?}", e))?;
    let mut available_devices: Vec<DeviceDoc> = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(|e| format!("Database error when trying to get all devices. Error: {:?}", e))? {
        available_devices.push(doc);
    }
    if let Some(idx) = available_devices.iter().position(|d| d.name == "orchestrator") {
        available_devices.remove(idx);
    }

    let mut assigned: Vec<AssignedStep> = Vec::with_capacity(sequence.len());
    for step in sequence.into_iter() {
        let func_name = &step.func;
        let module = step.module;

        // Verify the module actually exports the required function
        let has_func = module.exports.iter().any(|e| e.name == *func_name);
        if !has_func {
            return Err(format!(
                "Failed to find function '{}' from requested module: {}",
                func_name, module.name
            ));
        }

        // Either validate the user-specified device, or auto-pick one
        let chosen_device = if let Some(device) = step.device {
            if !device_satisfies_module(&device, &module) {
                return Err(format!(
                    "device '{}' does not satisfy module '{}' requirements",
                    device.name, module.name
                ));
            }
            device
        } else {
            // Select first device that satisfies modules requirements
            if let Some(device) = available_devices
                .iter()
                .find(|d| device_satisfies_module(d, &module))
                .cloned()
            {
                device
            } else {
                let reqs = serde_json::to_string_pretty(&module.requirements)
                    .unwrap_or_else(|_| "<requirements>".to_string());
                return Err(format!(
                    "no matching device satisfying all requirements:\n{}",
                    reqs
                ));
            }
        };
        assigned.push(AssignedStep {
            device: chosen_device,
            module: module,
            func: func_name.clone(),
        });
    }

    if assigned.is_empty() {
        return Err("Error on deployment: no steps assigned".into());
    }
    Ok(assigned)
}


/// Helper function that gathers necessary info about a module to build the "modules" section
/// for a DeploymentNode. Mainly the urls where the supervisor can fetch required files (wasm, models etc)
pub fn module_data(module: &ModuleDoc, package_base_url: &str) -> Result<DeviceModule, String> {
    let base = package_base_url.trim_end_matches('/');
    let mod_id = module.id.ok_or_else(|| "Module id missing".to_string())?;

    let binary = format!("{}/file/module/{}/wasm", base, mod_id);
    let description = format!("{}/file/module/{}/description", base, mod_id);
    let mut other: HashMap<String, String> = HashMap::new();
    if let Some(data_files) = module.data_files.as_ref() {
        for filename in data_files.keys() {
            let url = format!("{}/file/module/{}/{}", base, mod_id, filename);
            other.insert(filename.clone(), url);
        }
    }

    Ok(DeviceModule {
        id: mod_id,
        name: module.name.clone(),
        urls: DeviceModuleUrls { binary, description, other },
    })
}