use crate::lib::constants::{COLL_MODULE, MODULE_DIR, MOUNT_DIR, WASMIOT_INIT_FUNCTION_NAME};
use crate::lib::mongodb::{insert_one, get_collection};
use crate::api::module_cards::{delete_all_module_cards, delete_module_card_by_id};
use crate::structs::openapi::{OpenApiDocument, OpenApiEncodingObject, OpenApiFormat, OpenApiInfo, OpenApiMediaTypeObject, OpenApiOperation, OpenApiParameterEnum, OpenApiParameterIn, OpenApiParameterObject, OpenApiPathItemObject, OpenApiRequestBodyObject, OpenApiResponseObject, OpenApiSchemaEnum, OpenApiSchemaObject, OpenApiServerObject, OpenApiServerVariableObject, OpenApiTagObject, OpenApiVersion, RequestBodyEnum, ResponseEnum};
use actix_web::{web, HttpRequest, HttpResponse, Responder, Result};
use serde_json::{json, Value, Map};
use mongodb::bson::{self, Bson, doc, oid::ObjectId, Document};
use actix_multipart::Multipart;
use futures_util::stream::StreamExt;
use futures::stream::TryStreamExt;
use std::io::Write;
use std::path::Path;
use log::{error, warn, debug};
use serde::{Serialize, Deserialize};
use std::fs;
use std::collections::{HashMap, HashSet};
use actix_files::NamedFile;
use wasmparser::{ExternalKind, Parser, Payload, TypeRef, ValType as WValType};
use crate::structs::module::{
    ModuleDoc, WasmBinaryInfo, WasmExport, WasmRequirement
};
use crate::lib::errors::ApiError;


// TODO: Module updates (and their notifications if they are already deployed)

/// Contains a description of the received file, as well as where the file was saved to.
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadedFile {
    pub fieldname: String,
    pub originalname: String,
    pub filename: String,
    pub path: String,
    pub size: usize,
    pub mimetype: String,
}


/// Contains the values of the headers (fields), and the actual value in the multipart
/// field (value).
#[derive(Debug, Serialize, Deserialize)]
pub struct MultipartField {
    pub fieldname: String,
    pub filename: String, 
    pub mimetype: String,
    pub value: String
}


/// Structure to hold all fields, values and files received in a multipart request.
#[derive(Debug, Serialize, Deserialize)]
pub struct MultipartSummary {
    pub fields: Vec<MultipartField>,
    pub files: Vec<UploadedFile>,
}


/// Stores the name and type of a single parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionParam {
    /// Name of this parameter
    pub name: String,
    /// Type of this parameter
    #[serde(rename = "type")]
    pub ty: String,
}


/// Stores a single mount for a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    /// The media type of this mount (usually application/octet-stream)
    #[serde(rename = "mediaType")]
    pub media_type: String,
    /// The stage of this mount. Can be output, deployment or execution
    pub stage: String, // TODO: Limit what this can be.
}


/// Stores the specifications for a single function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    /// Http method used when calling this function (for example "get", "post", etc)
    pub method: String,
    /// List of parameters for this function. Uses the FunctionParam struct.
    pub parameters: Vec<FunctionParam>,
    /// List of mounts for this function. Uses the MountSpec struct.
    pub mounts: HashMap<String, MountSpec>,
    /// The output type of this function. Can be either a basic output type of a wasm module like
    /// an integer or a float, or something else.
    #[serde(rename = "outputType")]
    pub output_type: String,
}


/// This function is meant to handle multipart requests that might or might not
/// contain multiple files and fields. It processes the request body, extracts the
/// separate fields into json, and saves files to disk while adding saved file information
/// on the returned json as well.
async fn handle_multipart_request(mut payload: Multipart) -> Result<MultipartSummary, ApiError> {

    // Ensure the module directory exists
    if let Err(e) = std::fs::create_dir_all(MODULE_DIR) {
        error!("❌ Failed to create module directory: {}", e);
        return Err(ApiError::internal_error("Failed to create module directory"));
    }

    // Iterate over each field in the multipart payload
    let mut summary = MultipartSummary {
        fields: Vec::new(),
        files: Vec::new(),
    };
    while let Some(Ok(mut field)) = payload.next().await {

        let mut multipart_field = MultipartField {
            fieldname: String::new(),
            filename: String::new(),
            mimetype: String::new(),
            value: String::new()
        };

        // Extract field metadata
        let content_disposition = field.content_disposition();
        let name = content_disposition
            .and_then(|cd| cd.get_name())
            .unwrap_or("")
            .to_string();
        let filename = content_disposition
            .and_then(|cd| cd.get_filename())
            .unwrap_or("")
            .to_string();
        let mimetype = field.content_type()
            .map(|mime| mime.essence_str())
            .unwrap_or("")
            .to_string();

        // Ignore fields that have no name set
        if name == "" {
            warn!("⚠️ Ignoring a multipart field with no name");
            continue;
        }

        // If field has no content type, assume its a plain text field.
        // Assume all other fields with a mimetype are related to file uploads.
        if mimetype.is_empty() {
            let mut bytes = web::BytesMut::new();
            while let Some(Ok(chunk)) = field.next().await {
                bytes.extend_from_slice(&chunk);
            }
            let value = String::from_utf8_lossy(&bytes).to_string();
            debug!("📄 Received module field: {} = {}", name, value);
            multipart_field.fieldname = name;
            multipart_field.filename = filename;
            multipart_field.mimetype = mimetype;
            multipart_field.value = value;
            summary.fields.push(multipart_field);
            continue;
        }

        // If the field has content type of application/wasm, save the file to a different 
        // folder than other mounts
        let ext = std::path::Path::new(&filename)
            .extension().and_then(|s| s.to_str()).unwrap_or("");
        let saved_name = if ext.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            format!("{}.{}", uuid::Uuid::new_v4(), ext)
        };
        let base_dir = if mimetype == "application/wasm" { MODULE_DIR } else { MOUNT_DIR };
        let filepath = format!("{}/{}", base_dir, saved_name);

        // Ensure directory exists (create it if missing)
        if let Err(e) = std::fs::create_dir_all(base_dir) {
            error!("❌ Failed to ensure upload directory '{}': {}", base_dir, e);
            return Err(ApiError::internal_error("Failed to prepare upload directory"));
        }

        let mut f = match std::fs::File::create(&filepath) {
            Ok(file) => file,
            Err(e) => {
                error!("❌ Failed to create file: {e}");
                return Err(ApiError::internal_error("Failed to create file to disk."));
            }
        };

        while let Some(Ok(chunk)) = field.next().await {
            if let Err(e) = f.write_all(&chunk) {
                error!("❌ Failed to write file: {e}");
                return Err(ApiError::internal_error("Failed to write file to disk."));
            }
        }

        let meta = match std::fs::metadata(&filepath) {
            Ok(m) => m,
            Err(e) => {
                error!("❌ Failed to get metadata for file '{}': {}", filepath, e);
                return Err(ApiError::internal_error("Failed to get file metadata"));
            }
        };

        debug!("📦 Saved file to disk: {}", filepath);
        let uploaded = UploadedFile {
            fieldname: name,         
            originalname: filename,
            filename: saved_name,
            path: filepath,
            size: meta.len() as usize,
            mimetype: if mimetype.is_empty() { "application/octet-stream".into() } else { mimetype }, // Default to application/octet-stream
        };
        summary.files.push(uploaded);

    }
    debug!("📦 Finished processing multipart payload, summary=\n{:?}", summary);
    return Ok(summary);

}


/// Creates a filter for module queries based on the provided string.
/// If the string is a valid ObjectId, it filters by `_id`, otherwise by `name`.
fn module_filter(x: &str) -> Document {
    match ObjectId::parse_str(x) {
        Ok(id) => doc! { "_id": id },
        Err(_) => doc! { "name": x },
    }
}


/// POST /file/module
/// 
/// Endpoint for creating a new module. Extracts the description and wasm module
/// from the request body, and returns the id of the newly created module entry.
pub async fn create_module(payload: Multipart) -> Result<impl Responder, ApiError> {
    // Ensure the target directory exists
    if let Err(e) = std::fs::create_dir_all(MODULE_DIR) {
        error!("❌ Failed to create module directory: {e}");
        return Err(ApiError::internal_error("Failed to create module directory"));
    }

    let summary = match handle_multipart_request(payload).await {
        Ok(s) => s,
        Err(e) => {
            error!("❌ Failed to process multipart request: {}", e);
            return Err(ApiError::internal_error("Failed to process multipart request"));
        }
    };

    // Get the first file that is a wasm module
    let wasm_upload = match summary.files.iter().find(|f| f.mimetype == "application/wasm") {
        Some(file) => file,
        None => return Err(ApiError::bad_request("No .wasm file provided")),
    };
    // Get the user defined wasm module name
    let module_name = match summary.fields.iter().find(|f| f.fieldname == "name") {
        Some(field) => field.value.clone(),
        None => return Err(ApiError::bad_request("No module name provided")),
    };
    // Get the name (filename) of the uploaded wasm module
    let wasm_filename = wasm_upload.originalname.clone();
    // Get the file path
    let wasm_file_path = wasm_upload.path.clone();
    // Get the user defined module name
    let name = module_name.clone();

    // Get the exports and requirements from the wasm module
    let (requirements, exports) = match parse_wasm_at_path(&wasm_file_path) {
        Ok(x) => x,
        Err(e) => {
            error!("❌ Failed to parse wasm at '{}': {}", wasm_file_path, e);
            return Err(ApiError::bad_request("Failed to parse wasm module"));
        }
    };


    let wasm_metadata = WasmBinaryInfo {
        original_filename: wasm_filename,
        file_name: wasm_upload.filename.clone(),
        path: wasm_file_path
    };    

    // Other values are updated after user uploads the module description, for now they are empty
    let wasm_doc = ModuleDoc {
        id: None,
        name: name,
        exports,
        requirements,
        wasm: wasm_metadata,
        data_files: None,
        description: None,
        mounts: None,
        is_core_module: false,
    };

    let wasm_document = bson::to_document(&wasm_doc).unwrap();
    debug!("📄 Final module document before saving:\n{:?}", wasm_document);
    // Save the document to the database
    let inserted_id = insert_one(COLL_MODULE, &wasm_document).await;
    let module_id = match inserted_id {
        Ok(Bson::ObjectId(id)) => id,
        _ => {
            error!("❌ Failed to convert the id returned by mongodb into an objectId: {:?}", inserted_id);
            return Err(ApiError::db("Database failure, check server logs"));
        }
    };
    debug!("✅ Module document saved to database, _id={:?}", module_id);    

    Ok(HttpResponse::Created().json(json!({ "id": module_id.to_hex() })))

}


/// Parses a wasm module into imports and exports. Reads the module from the given path.
fn parse_wasm_at_path(
    path: &str,
) -> Result<(Vec<WasmRequirement>, Vec<WasmExport>), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let mut requirements: Vec<WasmRequirement> = Vec::new();
    let mut exports: Vec<WasmExport> = Vec::new();

    // Entries from the Type section of the module. Contains the different types present in the wasm module.
    let mut types: Vec<wasmparser::CompositeInnerType> = Vec::new();
    // List of type indexes (pointing to the type section) of all imported functions.
    let mut import_func_types: Vec<u32> = Vec::new();
    // List of type indexes for all local, non-imported functions. Points to type section.
    let mut local_func_types: Vec<u32> = Vec::new();

    // Iterate through each section of the wasm module, reading the type, import, function and export sections.
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload? {

            // Extract the types from Type Section  of the wasm file, and save them into 
            // an array for later. Type Section is before Import/Function/Export sections 
            // in wasm files, so we can rely on types array being populated before its 
            // needed in other section here.
            Payload::TypeSection(reader) => {
                for fty in reader {
                    let ty = fty?;
                    let thing = ty.types();
                    for item in thing {
                        let composite_type = item.composite_type.clone();
                        let composite_inner_type = composite_type.inner;
                        types.push(composite_inner_type);
                    }
                }
            }

            // Extract imports, and use the types list to figure out which
            // import has what kind of parameter/result types.
            Payload::ImportSection(reader) => {
                for item in reader {
                    let imp = item?;
                    if let TypeRef::Func(type_index) = imp.ty {
                        import_func_types.push(type_index);
                        
                        // Check that index isnt out of bounds on the types array
                        if (type_index as usize) >= types.len() {
                            warn!("During module parsing, index was out of bounds in ImportSection");
                            continue;
                        }
                        // Check that the composite type referred by index is of type FuncType
                        if let Some(composite_inner_type) = types.get(type_index as usize) {
                            match composite_inner_type {
                                wasmparser::CompositeInnerType::Func(f) => {
                                    requirements.push(WasmRequirement {
                                        module: imp.module.to_string(),
                                        name: imp.name.to_string(),
                                        kind: "function".to_string(),
                                        params: f.params().iter().map(wasmparser_valtype).collect(),
                                        results: f.results().iter().map(wasmparser_valtype).collect(),
                                    });
                                },
                                wasmparser::CompositeInnerType::Array(_a) => {
                                    warn!("Import referenced a array type. That is not supported by current orchestrator. (The exact referenced type was: {:?})", composite_inner_type);
                                    continue;
                                }
                                _ => {
                                    debug!("Import referenced a type that was not functype or arraytype. The referenced type was: {:?}", composite_inner_type);
                                    continue;
                                }
                            }
                        }
                        else {
                            error!("Failed to find the type from type array even though index was not out of bounds.");
                            continue;
                        }
                    }
                }
            }

            // Function Section contains references to the Types Section. It basically
            // maps out which function has what parameters/results.
            // Function section is before export section in wasm files, so we can rely on
            // local_func_types being populated before its needed in export section.
            Payload::FunctionSection(reader) => {
                for ty_idx in reader {
                    local_func_types.push(ty_idx?);
                }
            }

            // Export Section contains information on all exports of the wasm file.
            // Uses the import_func_types and local_func_types to figure out the types 
            // of each exports parameters/results. Note that imported functions can 
            // apparently be also exported, which is why both arrays are needed here.
            Payload::ExportSection(reader) => {
                for item in reader {
                    let ex = item?;
                    if ex.kind == ExternalKind::Func {
                        let func_idx = ex.index as usize;
                        let type_index = if func_idx < import_func_types.len() {
                            import_func_types[func_idx] as usize
                        } else {
                            let local_idx = func_idx - import_func_types.len();
                            if local_idx >= local_func_types.len() {
                                error!("Index was out of bounds when trying to get import function types! Index was: {:?}, and the length of local_func_types was: {:?}", local_idx, local_func_types.len());
                            }
                            local_func_types[local_idx] as usize
                        };

                        if let Some(composite_inner_type) = types.get(type_index) {
                            match composite_inner_type {
                                wasmparser::CompositeInnerType::Func(f) => {
                                    exports.push(WasmExport {
                                        name: ex.name.to_string(),
                                        parameter_count: f.params().len(),
                                        params: f.params().iter().map(wasmparser_valtype).collect(),
                                        results: f.results().iter().map(wasmparser_valtype).collect(),
                                    });
                                },
                                wasmparser::CompositeInnerType::Array(_a) => {
                                    warn!("Export referenced a array type. That is not supported by current orchestrator. (The exact referenced type was: {:?})", composite_inner_type);
                                    continue;
                                }
                                _ => {
                                    debug!("Export referenced a type that was not functype or arraytype. The referenced type was: {:?}", composite_inner_type);
                                    continue;
                                }
                            }
                        }
                    } else {
                        debug!("Ignored an export that wasn't a func. Instead it had type: {:?}", ex.kind);
                    }
                }
            }
            _ => {}
        }
    }
    debug!("Wasm reading results:\n{:?}\n\n{:?}", requirements, exports);
    Ok((requirements, exports))
}


/// Helper function for converting a wasmparsers valtype into a string.
fn wasmparser_valtype(t: &WValType) -> String {
    match t {
        WValType::I32 => "i32".to_string(),
        WValType::I64 => "i64".to_string(),
        WValType::F32 => "f32".to_string(),
        WValType::F64 => "f64".to_string(),
        WValType::V128 => "v128".to_string(),
        _ => format!("{:?}", t),
    }
}


/// Helper function for collecting paths to all mounted files related to a single module
fn collect_datafile_paths(doc: &ModuleDoc) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(data_files) = &doc.data_files {
        for (_k, v) in data_files.iter() {
            out.push(v.path.clone());
        }
    }
    out
}


/// Helper function for deleting files related to a single module
fn try_delete_file(path: &str, files_deleted: &mut usize, file_errors: &mut Vec<String>) {
    match fs::remove_file(path) {
        Ok(()) => {
            debug!("🗑️ Deleted file: {}", path);
            *files_deleted += 1;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("File already deleted or doesn't exist: {}", path);
        }
        Err(e) => {
            warn!("Failed to delete file '{}': {}", path, e);
            file_errors.push(format!("{}: {}", path, e));
        }
    }
}


/// Helper function for deleting all files in a single folder 
/// (for purposes of deleting all modules and their files)
fn delete_all_files_in_dir(dir: &str) -> (usize, Vec<String>) {
    let mut deleted = 0usize;
    let mut errors = Vec::new();

    // Get every item in a given directory
    let path = Path::new(dir);
    let entries = match fs::read_dir(path) {
        Ok(it) => it,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!("read_dir('{}'): {}", dir, e));
            }
            return (deleted, errors);
        }
    };

    // Iterate over each item, deleting them if they are files (but not if they are folders etc)
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => { errors.push(format!("iterating '{}': {}", dir, e)); continue; }
        };

        let p = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => { errors.push(format!("file_type '{}': {}", p.display(), e)); continue; }
        };

        if file_type.is_file() || file_type.is_symlink() {
            match fs::remove_file(&p) {
                Ok(()) => { debug!("🗑️ deleted {}", p.display()); deleted += 1; }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    debug!("already missing (ok): {}", p.display());
                }
                Err(e) => { errors.push(format!("remove_file '{}': {}", p.display(), e)); }
            }
        } else {
            debug!("skipping non-file in {}: {}", dir, p.display());
        }
    }

    (deleted, errors)
}


/// DELETE /file/module
/// 
/// Endpoint for deleting all modules. Also removes related modulecards, wasm modules and mounted files.
pub async fn delete_all_modules() -> Result<impl Responder, ApiError> {

    // Delete all module docs from database
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let deleted = match coll.delete_many(doc! {}).await {
        Ok(res) => res.deleted_count,
        Err(e) => {
            error!("Failed to delete module documents: {e}");
            return Err(ApiError::internal_error("Failed to delete module documents"));
        }
    };

    // Delete all wasm files and mounted files
    let (wasm_deleted, mut wasm_errs) = delete_all_files_in_dir(MODULE_DIR);
    debug!("wasm files deleted: {}, errors: {:?}", wasm_deleted, wasm_errs);
    let (mounts_deleted, mounts_errs) = delete_all_files_in_dir(MOUNT_DIR);
    debug!("mount files deleted: {}, errors: {:?}", mounts_deleted, mounts_errs);
    wasm_errs.extend(mounts_errs);

    // Delete all module cards
    let _ = delete_all_module_cards().await;

    Ok(HttpResponse::Ok().json(json!({
        "message": "Deleted all modules",
        "docs_deleted": deleted,
        "files_deleted": wasm_deleted + mounts_deleted,
        "file_errors": wasm_errs
    })))
}


/// DELETE /file/module/{module_id}
/// 
/// Deletes a single module by its id or name. Also removes all files related to it.
pub async fn delete_module_by_id(path: web::Path<String>) -> Result<impl Responder, ApiError> {
    let key = path.into_inner();
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;

    // Get the module document
    let filter = module_filter(&key);
    let doc_opt = match coll.find_one(filter.clone()).await {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to find module document to delete '{}': {}", key, e);
            return Err(ApiError::db(format!("Failed to search for module: {:?}", e)));
        }
    };

    // Return error if no module matched the query (id or name)
    let Some(doc) = doc_opt else {
        return Err(ApiError::not_found(format!("Module not found for query: {}", key)));
    };

    // Get the modules id
    let module_oid_hex = match doc.id {
        Some(oid) => oid.to_hex(),
        None => {
            error!("Module document missing valid id! Document: {:?}", doc);
            return Err(ApiError::internal_error("Module document missing valid id!"));
        }
    };

    // Delete related module card if id was found
    if !module_oid_hex.is_empty() {
        let _ = delete_module_card_by_id(web::Path::<String>::from(module_oid_hex.clone())).await;
    }

    // Delete all files related to the module
    let wasm_path = doc.wasm.path.clone();
    let mut files_deleted = 0usize;
    let mut file_errors: Vec<String> = Vec::new();
    try_delete_file(&wasm_path, &mut files_deleted, &mut file_errors);
    for p in collect_datafile_paths(&doc) {
        try_delete_file(&p, &mut files_deleted, &mut file_errors);
    }

    // Delete the module doc
    match coll.delete_one(filter).await {
        Ok(res) if res.deleted_count == 1 => Ok(HttpResponse::Ok().json(json!({
            "message":"Module deleted",
            "query": key,
            "files_deleted": files_deleted,
            "file_errors": file_errors
        }))),
        Ok(_) => Err(ApiError::not_found(format!("Module not found during delete, query: {}", key))),
        Err(e) => {
            error!("Failed to delete module doc '{}': {}", key, e);
            Err(ApiError::internal_error(format!("Failed to delete module document, query: {}", key)))
        }
    }
}


/// GET /file/module
/// 
/// Endpoint for getting all module docs from database
pub async fn get_all_modules() -> Result<impl Responder, ApiError> {
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let mut cursor = match coll.find(doc! {}).await {
        Ok(c) => c,
        Err(e) => {
            error!("Error querying modules: {}", e);
            return Err(ApiError::db(format!("Error querying modules: {}", e)));
        }
    };
    let mut out: Vec<ModuleDoc> = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(ApiError::db)? {
        out.push(doc);
    }
    let mut v = serde_json::to_value(&out).map_err(ApiError::internal_error)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}


/// GET /file/module/{module_id}
/// 
/// Endpoint for getting one module doc by its name/id from database.
pub async fn get_module_by_id(path: web::Path<String>) -> Result<impl Responder, ApiError> {
    let id_str = path.into_inner();
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let filter = module_filter(&id_str);
    match coll.find_one(filter).await {
        Ok(Some(doc)) => {
            let mut v = serde_json::to_value(&doc).map_err(ApiError::internal_error)?;
            crate::lib::utils::normalize_object_ids(&mut v);
            Ok(HttpResponse::Ok().json(vec![v]))
        }
        Ok(None) => Ok(HttpResponse::Ok().json(Vec::<Document>::new())), // []
        Err(e) => Ok(HttpResponse::InternalServerError().json(json!({
            "message": "Error querying module",
            "error": e.to_string()
        }))),
    }
}


/// POST /file/module/{module_id}/upload
/// 
/// Endpoint that takes the module description as an html form (multipart request), and
/// creates an openapi documentation for the related module from that. 
/// Note that this expects the form to have a very specific format.
pub async fn describe_module(
    path: web::Path<String>,
    payload: Multipart,
) -> Result<impl Responder, ApiError> {

    // TODO: Switch to using json instead of multipart for sending descriptions. That way you can have some clear
    // definition of what the description should contain (easy to update etc).

    // -------------- Start of multipart/description parsing -----------------

    // Get the multipart summary first. This is helpful because handle_multipart_request
    // handles correctly saving files that came with the request.
    let summary = match handle_multipart_request(payload).await {
        Ok(s) => s,
        Err(e) => {
            error!("❌ multipart handling failed: {e}");
            return Err(ApiError::internal_error("Failed to process multipart"));
        }
    };

    // After handling the incoming multipart request, find the module that the mounts and description
    // that were sent with the request are related to. Fail miserably if the module is not found.
    let key = path.into_inner();
    let filter = module_filter(&key);
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let module_doc = match coll.find_one(filter.clone()).await {
        Ok(Some(d)) => d,
        Ok(None) => return Err(ApiError::not_found("Module not found")),
        Err(e) => {
            error!("Database error when searching for a module related to module description: {e}");
            return Err(ApiError::internal_error("Database error"));
        }
    };
    let module_name = module_doc.name.clone();

    // Parse the description field by field
    let description_json = {

        // Attempt to build the module description field by field from the multipart summary.
        // Summary was built from fields that have names/values with brackets like the below example:
        //
        // take_image[method]       = GET
        // take_image[param0]       = integer
        // take_image[param1]       = integer
        // take_image[output]       = integer
        // take_image_predefined_path[mounts][0][name]  = image.jpeg
        // take_image_predefined_path[mounts][0][stage] = output
        //
        // In general, the parsing here supports field names with following formats:
        // func[paramN], func[method], func[output],
        // func[mounts][<idx>][name] and func[mounts][<idx>][stage]
        // Others are not supported and will be ignored.

        // Empty map to contain values we are about to collect.
        let mut root: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
        // A hashmap meant to temporarily store information on mounts.
        // Information: <function_name, Vec<(mount array index, field name, field value)>
        let mut mounts_acc: HashMap<String, Vec<(usize, String, String)>> = HashMap::new();

        // Iterate over every field in the multipart summary.
        // Fields related to mounts are handled differently from others
        for field in &summary.fields {
            if !field.mimetype.is_empty() { continue; }
            let name = field.fieldname.as_str();

            // First check that the name contains a starting bracket, get its location, and also check there is 
            // an ending bracket.
            if let (Some(l), true) = (name.find('['), name.ends_with(']')) {

                // Get functions name, which is the string preceding first bracket
                let func = &name[..l];
                // Get the part following the first bracket. For example, "mounts][0][name" or "param0"
                let inner = &name[l + 1 .. name.len() - 1];

                // Handle the case where the field concerns mounts (has the substring "mounts][" in it)
                if let Some(rest) = inner.strip_prefix("mounts][") {

                    // Get the mount array index from the name, and check that its a valid index (usize)
                    if let Some((idx_str, key_with_br)) = rest.split_once("][") {
                        if let Ok(idx) = idx_str.parse::<usize>() {

                            // Get the final key from the name. If the field was named
                            // take_image_predefined_path[mounts][0][name] the final key would be "name".
                            // Save the information to the temporary mounts hashmap.
                            let key = key_with_br.trim_end_matches(']');
                            mounts_acc.entry(func.to_string())
                                .or_default()
                                .push((idx, key.to_string(), field.value.clone()));
                            continue;
                        }
                    }
                }

                // Handle the case where the field didnt concern mounts
                // Examples of this are fields with param0, param1, method, output etc...
                root.entry(func.to_string())
                    .or_default()
                    .insert(inner.to_string(), Value::String(field.value.clone()));
            }
        }

        // Iterate over the temporary mount hashmap, and add them to the "root" object correctly.
        for (func, triples) in mounts_acc {

            // Create a sufficiently large array for all mounts
            let max_idx = triples.iter().map(|(i,_,_)| *i).max().unwrap_or(0);
            let mut items = vec![serde_json::Map::new(); max_idx + 1];

            // Insert the mount information to correct places in the array. (Based on the mount indexes)
            for (i, k, v) in triples {
                items[i].insert(k, Value::String(v));
            }

            // Add all mounts under the "mounts" key in the "root" object.
            root.entry(func)
                .or_default()
                .insert("mounts".into(), Value::Array(items.into_iter().map(Value::Object).collect()));
        }

        // If root object was empty, something was wrong with the request.
        if root.is_empty() {
            return Err(ApiError::bad_request("No description was provided, or description was malformed."));
        }
        serde_json::to_value(root).unwrap()
    };

    // Go through all files in the multipart summary, and store them under their names 
    // only if they are NOT wasm files.
    let files_by_field: HashMap<String, &crate::api::module::UploadedFile> = summary
        .files
        .iter()
        .filter(|f| f.mimetype != "application/wasm") 
        .map(|f| (f.fieldname.clone(), f))
        .collect();

    // Go through the function/mount descriptions created earlier, and turn them into a map
    // of their names and FunctionSpec objects.
    let mut functions: HashMap<String, FunctionSpec> = HashMap::new();
    let obj = description_json.as_object().cloned().unwrap_or_default();
    for (func_name, func_val) in obj.into_iter() {
        if !func_val.is_object() { continue; }
        let fobj = func_val.as_object().unwrap();

        // Get the method, or use "get" as default. All methods must be lowercase.
        let method = fobj.get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_lowercase();

        // Store all parameters (fields with names starting with "param") as FunctionParams
        let mut params: Vec<FunctionParam> = Vec::new();
        for (k, v) in fobj.iter() {
            if k.starts_with("param") {
                let ty = v.as_str().unwrap_or("string").to_string();
                params.push(FunctionParam { name: k.clone(), ty });
            }
        }
        params.sort_by_key(|p| p.name.clone());

        // Create a hashmap of MountSpecs from the description and files_by_field.
        let mut mounts = HashMap::new();
        if let Some(arr) = fobj.get("mounts").and_then(Value::as_array) {
            for m in arr {
                let m_name  = m.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let m_stage = m.get("stage").and_then(Value::as_str).unwrap_or("").to_string(); // <- NOT "deployment"
                if m_name.is_empty() { continue; }
                let media = files_by_field
                    .get(&m_name)
                    .map(|f| f.mimetype.clone())
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                mounts.insert(m_name, MountSpec { media_type: media, stage: m_stage });
            }
        } 

        // Get the output type for the current function. Check through this functions MountSpecs for any mounts that
        // have type "output", and get its mediatype, if present. Defaults into application/octet-stream in most cases.
        // Works on the assumption that a function only has one output mount.
        // TODO: Can a function have multiple output mounts?
        let output_field = fobj.get("output").and_then(Value::as_str).map(|s| s.to_string());
        let output_type = if let Some(mt) = functions_output_mount_mediatype(&mounts) {
            if !(&mt.eq_ignore_ascii_case("application/octet-stream")) { mt } else {
                output_field.clone().unwrap_or_else(|| "application/octet-stream".to_string())
            }
        } else {
            output_field.clone().unwrap_or_else(|| "application/octet-stream".to_string())
        };

        functions.insert(func_name, FunctionSpec { method, parameters: params, mounts, output_type });
    }

    // Get a list of mounts that are missing (specifically, mounts that refer to files that are missing)
    // This concerns only deployment mounts, since their files are required to be present before module execution.
    let mut missing: Vec<(String, String)> = Vec::new();
    for (fname, fspec) in &functions {
        for (mname, mspec) in &fspec.mounts {
            if mspec.stage == "deployment" && !files_by_field.contains_key(mname) {
                missing.push((fname.clone(), mname.clone()));
            }
        }
    }

    // Return an error if mount is missing, unless it is related to some specific wasmiot init function
    // (Not sure if that check is actually necessary, but its needed for 1:1 compatibility with original
    // JS version of the orchestrator)
    if !missing.is_empty() {
        if let Some(init_f) = functions.get(WASMIOT_INIT_FUNCTION_NAME) {
            let init_mount_names: HashSet<&str> = init_f.mounts.keys().map(|s| s.as_str()).collect();
            let mut actually_missing = Vec::new();
            for (fname, mname) in missing.into_iter() {
                if !init_mount_names.contains(mname.as_str()) {
                    actually_missing.push((fname, mname));
                } else {
                    debug!("NOTE: '{}' missing mount '{}', but this is ignored because of the wasmiot init function exception.", fname, mname);
                }
            }
            if !actually_missing.is_empty() {
                return Err(ApiError::bad_request(format!("Functions missing mounts: {}", serde_json::to_string(&actually_missing).unwrap_or_default())));
            }
        } else {
            return Err(ApiError::bad_request(format!("Functions missing mounts: {}", serde_json::to_string(&missing).unwrap_or_default())));
        }
    }

    // -------------- End of multipart/description parsing -----------------

    // TODO: When you switch away from multipart requests, change this part too.
    // Generate a listing of all datafiles related to this module
    let mut update_doc = Document::new();
    for f in summary.files.iter().filter(|f| f.mimetype != "application/wasm") {
        let sub = doc! {
            "originalFilename": &f.originalname,
            "fileName": &f.filename,
            "path": &f.path,
        };
        update_doc.insert(format!("dataFiles.{}", f.fieldname), Bson::Document(sub));
    }

    // Generate a mount list in correct format to be stored to database
    let mounts_json = mounts_from_functions(&functions);
    let mounts_doc: Document = bson::to_document(&mounts_json).unwrap_or_else(|_| Document::new());
    update_doc.insert("mounts", Bson::Document(mounts_doc));

    // Generate the openapi description in correct format to be stored to database
    let openapi_json = module_endpoint_descriptions(&module_name, &functions);
    let description_doc: Document = bson::to_document(&openapi_json).unwrap_or_else(|_| Document::new());
    update_doc.insert("description", Bson::Document(description_doc));

    // Update the entry related to the current module with the openapi description, mount listing and datafile list.
    let update = doc! { "$set": update_doc };
    if let Err(e) = coll.update_many(filter, update).await {
        error!("Failed to update module with mounts/description: {e}");
        return Err(ApiError::db("update failed"));
    }
    Ok(HttpResponse::Ok().json(json!({ "description": openapi_json })))
}


/// Creates an openapi descriptions from module name and a list of functions and their specs
pub fn module_endpoint_descriptions(
    module_name: &str,
    functions: &HashMap<String, FunctionSpec>,
) -> OpenApiDocument {

    let deployment_param = OpenApiParameterEnum::OpenApiParameterObject(OpenApiParameterObject {
        name: "deployment".into(),
        r#in: OpenApiParameterIn::Path,
        description: Some("Deployment ID".into()),
        required: true,
        deprecated: None,
        allow_empty_value: None,
        style: None,
        explode: None,
        allow_reserved: None,
        schema: Some(OpenApiSchemaEnum::OpenApiSchemaObject(OpenApiSchemaObject {
            r#type: Some("string".into()),
            properties: None,
            format: None
        })),
        content: None,
    });

    let mut paths: HashMap<String, OpenApiPathItemObject> = HashMap::new();

    for (func_name, func) in functions {
        let func_params: Vec<OpenApiParameterEnum> = func.parameters.iter().map(|p| {
            OpenApiParameterEnum::OpenApiParameterObject(OpenApiParameterObject {
                name: p.name.clone(),
                r#in: OpenApiParameterIn::Query,
                description: Some("Auto-generated description".into()),
                required: true,
                deprecated: None,
                allow_empty_value: None,
                style: None,
                explode: None,
                allow_reserved: None,
                schema: Some(OpenApiSchemaEnum::OpenApiSchemaObject(OpenApiSchemaObject {
                    r#type: Some(p.ty.clone()),
                    properties: None,
                    format: None
                })),
                content: None,
            })
        }).collect();

        let mut content: HashMap<String, OpenApiMediaTypeObject> = HashMap::new();
        if is_primitive(&func.output_type) {
            content.insert(
                "application/json".into(),
                OpenApiMediaTypeObject {
                    schema: Some(OpenApiSchemaEnum::OpenApiSchemaObject(OpenApiSchemaObject {
                        r#type: Some(func.output_type.clone()),
                        properties: None,
                        format: None
                    })),
                    encoding: None
                }
            );
        } else {
            content.insert(
                func.output_type.clone(),
                OpenApiMediaTypeObject {
                    schema: Some(OpenApiSchemaEnum::OpenApiSchemaObject(OpenApiSchemaObject {
                        r#type: Some("string".into()),
                        properties: None,
                        format: Some(OpenApiFormat::Binary)
                    })),
                    encoding: None
                }
            );
        }

        let mut responses: HashMap<String, ResponseEnum> = HashMap::new();
        responses.insert(
            "200".into(),
            ResponseEnum::OpenApiResponseObject(OpenApiResponseObject {
                description: "Auto-generated description of response".into(),
                headers: None,
                content: Some(content),
                links: None
            })
        );

        let input_mounts: Vec<(&String, &MountSpec)> = func
            .mounts
            .iter()
            .filter(|(_name, m)| !m.stage.eq_ignore_ascii_case("output"))
            .collect();

        let request_body = if !input_mounts.is_empty() {
            let mut properties: HashMap<String, OpenApiSchemaEnum> = HashMap::new();
            let mut encoding: HashMap<String, OpenApiEncodingObject> = HashMap::new();

            for (name, m) in &input_mounts {
                properties.insert(
                    (*name).clone(),
                    OpenApiSchemaEnum::OpenApiSchemaObject(OpenApiSchemaObject {
                        r#type: Some("string".into()),
                        properties: None,
                        format: Some(OpenApiFormat::Binary),
                    })
                );
                encoding.insert(
                    (*name).clone(),
                    OpenApiEncodingObject {
                        content_type: Some(m.media_type.clone()),
                        headers: None,
                        style: None,
                        explode: None,
                        allow_reserved: None
                    }
                );
            }

            let mut mt_map: HashMap<String, OpenApiMediaTypeObject> = HashMap::new();
            mt_map.insert(
                "multipart/form-data".into(),
                OpenApiMediaTypeObject {
                    schema: Some(OpenApiSchemaEnum::OpenApiSchemaObject(OpenApiSchemaObject {
                        r#type: Some("object".into()),
                        properties: Some(properties),
                        format: None
                    })),
                    encoding: Some(encoding)
                }
            );

            Some(RequestBodyEnum::OpenApiRequestBodyObject(OpenApiRequestBodyObject {
                description: None,
                content: mt_map,
                required: Some(true)
            }))
        } else {
            None
        };

        let operation = OpenApiOperation {
            tags: vec![],
            summary: Some("Auto-generated description of function call method".into()),
            description: None,
            external_docs: None,
            operation_id: None,
            parameters: if func_params.is_empty() { None } else { Some(func_params) },
            request_body,
            responses,
            callbacks: None,
            deprecated: None,
            security: None,
            servers: None
        };

        let mut path_item = OpenApiPathItemObject {
            r#ref: None,
            summary: Some("Auto-generated description of function".into()),
            description: None,
            get: None, put: None, post: None, delete: None, options: None, head: None, patch: None, trace: None,
            servers: None,
            parameters: Some(vec![deployment_param.clone()])
        };

        match func.method.as_str() {
            "get"    => path_item.get    = Some(operation),
            "post"   => path_item.post   = Some(operation),
            "put"    => path_item.put    = Some(operation),
            "delete" => path_item.delete = Some(operation),
            "patch"  => path_item.patch  = Some(operation),
            "head"   => path_item.head   = Some(operation),
            "options"=> path_item.options= Some(operation),
            "trace"  => path_item.trace  = Some(operation),
            _ => path_item.get = Some(operation),
        }

        let path = supervisor_execution_path(module_name, func_name);
        paths.insert(path, path_item);
    }

    let mut servers: Vec<OpenApiServerObject> = Vec::new();
    servers.push(OpenApiServerObject {
        url: "http://{serverIp}:{port}".into(),
        description: None,
        variables: Some({
            let mut vars = HashMap::new();
            vars.insert(
                "serverIp".into(),
                OpenApiServerVariableObject {
                    r#enum: None,
                    default: "localhost".into(),
                    description: Some("IP or name found with mDNS of the machine running supervisor".into())
                }
            );
            vars.insert(
                "port".into(),
                OpenApiServerVariableObject {
                    r#enum: Some(vec!["5000".into(), "80".into()]),
                    default: "5000".into(),
                    description: None
                }
            );
            vars
        })
    });

    let tags = Some(vec![OpenApiTagObject {
        name: "WebAssembly".into(),
        description: Some("Executing WebAssembly functions".into()),
        external_docs: None
    }]);

    OpenApiDocument {
        openapi: OpenApiVersion::V3_0_3,
        info: OpenApiInfo {
            title: module_name.into(),
            description: Some("Calling microservices defined as WebAssembly functions".into()),
            terms_of_service: None,
            contact: None,
            license: None,
            version: "0.0.1".into()
        },
        servers: Some(servers),
        paths,
        components: None,
        security: None,
        tags,
        external_docs: None
    }

}


/// Helper function that makes an object containing functions and their related mounts in expected format.
pub fn mounts_from_functions(functions: &HashMap<String, FunctionSpec>) -> Value {
    let mut m = Map::new();
    for (func_name, func) in functions {
        m.insert(func_name.clone(), serde_json::to_value(&func.mounts).unwrap_or(json!({})));
    }
    Value::Object(m)
}


/// Helper function that returns a placeholder execution path that would be used on the supervisor
fn supervisor_execution_path(module_name: &str, func_name: &str) -> String {
    format!("/{{deployment}}/modules/{}/{}", module_name, func_name)
}


/// Helper function that returns if the type matches integer or float
fn is_primitive(ty: &str) -> bool {
    matches!(ty, "integer" | "float")
}


/// Helper function that returns the media type of the first mount that is an output mount
fn functions_output_mount_mediatype(mounts: &std::collections::HashMap<String, MountSpec>) -> Option<String> {
    mounts.values()
        .find(|m| m.stage == "output")
        .map(|m| m.media_type.clone())
}


/// GET /file/module/{module_id}/description
/// 
/// Endpoint for getting a modules description by its id/name
pub async fn get_module_description_by_id(path: web::Path<String>) -> Result<HttpResponse, ApiError> {
    let id_str = path.into_inner();
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let filter = module_filter(&id_str);
    match coll.find_one(filter).await {
        Ok(Some(doc)) => {
            match &doc.description {
                Some(desc) => {
                    let mut v = serde_json::to_value(&desc).map_err(ApiError::internal_error)?;
                    crate::lib::utils::normalize_object_ids(&mut v);
                    Ok(HttpResponse::Ok().json(v))
                },
                None       => Ok(HttpResponse::Ok().json(serde_json::Value::Object(serde_json::Map::new()))),
            }
        }
        Ok(None) => {
            Err(ApiError::not_found(format!("Module not found, module id/name: {}", id_str)))
        }
        Err(e) => Err(ApiError::internal_error(format!("Error querying module: {}", e)))
    }
}


/// GET /file/module/{module_id}/{file_name}
/// 
/// Endpoint that returns a given modules datafile/mounted file based on the given name.
/// The name must match the key for that file in the database, not the actual filename it has
/// in the filesystem. For module, accepts either modules id, or its name.
pub async fn get_module_datafile(
    _req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<NamedFile, ApiError> {
    let (id_str, datafile_key) = path.into_inner();
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let filter = module_filter(&id_str);

    // Load module doc
    let doc_opt = match coll.find_one(filter).await {
        Ok(d) => d,
        Err(e) => return Err(ApiError::db(format!("Database error: {}", e))),
    };

    let doc = match doc_opt {
        Some(d) => d,
        None => return Err(ApiError::not_found("Module not found")),
    };

    // Get the datafiles section of module docs, if it exists.
    let df_map = match &doc.data_files {
        Some(m) => m,
        None => return Err(ApiError::not_found("No dataFiles for this module")),
    };

    // Get the correct datafile information, if the given key matches any.
    let file_obj = match df_map.get(&datafile_key) {
        Some(f) => f,
        None => return Err(ApiError::not_found("Datafile key not found")),
    };

    // Get the path to the datafile, if it exists in the filesystem.
    let path = &file_obj.path;

    // Guess the mimetype of the file and return the file as response
    let mut named = NamedFile::open(path)
        .map_err(|_| ApiError::not_found("File not found on disk"))?;

    let guessed = mime_guess::from_path(path)
        .first_or_octet_stream();
    named = named.set_content_type(guessed);
    Ok(named)
}


/// GET /file/module/{module_id}/wasm
/// 
/// Endpoint for returning a wasm module (the binary file itself) by a modules id or name
pub async fn get_module_wasm(
    _req: HttpRequest,
    path: web::Path<String>,
) -> Result<NamedFile> {
    let id_str = path.into_inner();
    let coll = get_collection::<ModuleDoc>(COLL_MODULE).await;
    let filter = module_filter(&id_str);

    // Get the path to the module
    let doc = coll
        .find_one(filter)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Module not found"))?;
    let wasm_info = &doc.wasm;
    let path = &wasm_info.path;

    // Return the module with content type set to application/wasm
    let mut named = NamedFile::open(path)
        .map_err(|_| actix_web::error::ErrorNotFound("Wasm file not found on disk"))?;
    let wasm_mime: mime_guess::mime::Mime = "application/wasm".parse().unwrap();
    named = named.set_content_type(wasm_mime);
    Ok(named)
}
