use crate::lib::constants::{MODULE_DIR, WASMIOT_INIT_FUNCTION_NAME, MOUNT_DIR};
use crate::lib::mongodb::{insert_one, get_collection};
use crate::api::module_cards::{delete_all_module_cards, delete_module_card_by_id};
use actix_web::{web, HttpRequest, HttpResponse, Result};
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

/// The metadata saved to database related to given wasm module.
#[derive(Debug, Serialize, Deserialize)]
pub struct WasmMetadata {
    #[serde(rename = "originalFilename")]
    pub original_filename: String,
    #[serde(rename = "fileName")]
    pub filename: String,
    pub path: String
}

/// Represents a single export from a WebAssembly module.
/// These are functions that can be called from outside the module.
#[derive(Debug, Serialize, Deserialize)]
pub struct WasmExport {
    pub name: String,
    #[serde(rename = "parameterCount")]
    pub parameter_count: usize,
    pub params: Vec<String>, // List of function parameter types as strings
    pub results: Vec<String>, // List of function types as strings
}

/// Represents a requirement for a WebAssembly module. Usually a function, its module, and its name.
/// That function is expected to be provided by the supervisor to the webassembly module.
#[derive(Debug, Serialize, Deserialize)]
pub struct WasmRequirement {
    pub module: String,
    pub name: String,
    pub kind: String,
    pub params: Vec<String>, // List of function parameter types as strings
    pub results: Vec<String>, // List of function result types as strings
}

/// Empty placeholder for datafiles
#[derive(Debug, Serialize, Deserialize)]
pub struct DataFiles { }

/// Empty placeholder for the openapi description
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenApiDescription { }

/// Empty placeholder for list of mounts
#[derive(Debug, Serialize, Deserialize)]
pub struct Mounts { }

/// Structure that represents the document related to the webassembly module.
/// The serialized version of this is saved to mongodb.
#[derive(Debug, Serialize, Deserialize)]
pub struct WasmDoc {
    pub name: String,
    pub exports: Vec<WasmExport>,
    pub requirements: Vec<WasmRequirement>,
    pub wasm: WasmMetadata,
    #[serde(rename = "dataFiles")]
    pub data_files: DataFiles,
    #[serde(rename = "description")]
    pub open_api_description: OpenApiDescription,
    pub mounts: Mounts,
    #[serde(rename = "isCoreModule", default)]  
    pub is_core_module: bool, // Default to false
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
async fn handle_multipart_request(mut payload: Multipart) -> Result<MultipartSummary, actix_web::Error> {

    // Ensure the module directory exists
    if let Err(e) = std::fs::create_dir_all(MODULE_DIR) {
        error!("❌ Failed to create module directory: {}", e);
        return Err(actix_web::error::ErrorInternalServerError("Failed to create module directory"));
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
            return Err(actix_web::error::ErrorInternalServerError("Failed to prepare upload directory"));
        }

        let mut f = match std::fs::File::create(&filepath) {
            Ok(file) => file,
            Err(e) => {
                error!("❌ Failed to create file: {e}");
                return Err(actix_web::error::ErrorInternalServerError("Failed to create file to disk."));
            }
        };

        while let Some(Ok(chunk)) = field.next().await {
            if let Err(e) = f.write_all(&chunk) {
                error!("❌ Failed to write file: {e}");
                return Err(actix_web::error::ErrorInternalServerError("Failed to write file to disk."));
            }
        }
        let meta = std::fs::metadata(&filepath)?;
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


/// Endpoint for creating a new module. Extracts the description and wasm module
/// from the request body, and returns the id of the newly created module entry.
pub async fn create_module(payload: Multipart) -> HttpResponse {
    // Ensure the target directory exists
    if let Err(e) = std::fs::create_dir_all(MODULE_DIR) {
        error!("❌ Failed to create module directory: {e}");
        return HttpResponse::InternalServerError().body("Failed to create module directory");
    }

    let summary = match handle_multipart_request(payload).await {
        Ok(s) => s,
        Err(e) => {
            error!("❌ Failed to process multipart request: {}", e);
            return HttpResponse::InternalServerError().body("Failed to process multipart request");
        }
    };

    // Get the first file that is a wasm module
    let wasm_upload = match summary.files.iter().find(|f| f.mimetype == "application/wasm") {
        Some(file) => file,
        None => return HttpResponse::BadRequest().body("No .wasm file provided"),
    };
    // Get the user defined wasm module name
    let module_name = match summary.fields.iter().find(|f| f.fieldname == "name") {
        Some(field) => field.value.clone(),
        None => return HttpResponse::BadRequest().body("No module name provided"),
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
            return HttpResponse::BadRequest().body("Failed to parse wasm module");
        }
    };


    let wasm_metadata = WasmMetadata {
            original_filename: wasm_filename,
            filename: wasm_upload.filename.clone(),
            path: wasm_file_path
        };    

    // Other values are updated after user uploads the module description, for now they are empty
    let wasm_doc = WasmDoc {
        name: name,
        exports,
        requirements,
        wasm: wasm_metadata,
        data_files: DataFiles {},
        open_api_description: OpenApiDescription {},
        mounts: Mounts {},
        is_core_module: false,
    };

    let wasm_document = bson::to_document(&wasm_doc).unwrap();
    debug!("📄 Final module document before saving:\n{:?}", wasm_document);
    // Save the document to the database
    let inserted_id = insert_one("module", &wasm_document).await;
    let module_id = match inserted_id {
        Ok(Bson::ObjectId(id)) => id,
        _ => {
            error!("❌ Failed to convert the id returned by mongodb into an objectId: {:?}", inserted_id);
            return HttpResponse::InternalServerError().body("Database failure, check server logs");
        }
    };
    debug!("✅ Module document saved to database, _id={:?}", module_id);    

    HttpResponse::Created().json(json!({ "id": module_id.to_hex() }))

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
fn collect_datafile_paths(doc: &mongodb::bson::Document) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(df) = doc.get_document("dataFiles") {
        for (_k, v) in df.iter() {
            if let Some(obj) = v.as_document() {
                if let Ok(p) = obj.get_str("path") {
                    out.push(p.to_string());
                }
            }
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
            debug!("File already deleted before: {}", path);
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


/// Endpoint for deleting all modules. Also removes related modulecards, wasm modules and mounted files.
pub async fn delete_all_modules() -> HttpResponse {

    // Delete all module docs from database
    let coll = get_collection::<Document>("module").await;
    let deleted = match coll.delete_many(doc! {}).await {
        Ok(res) => res.deleted_count,
        Err(e) => {
            error!("Failed to delete module documents: {e}");
            return HttpResponse::InternalServerError().json(json!({
                "message":"Failed to delete module documents"
            }));
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

    HttpResponse::Ok().json(json!({
        "message": "Deleted all modules",
        "docs_deleted": deleted,
        "files_deleted": wasm_deleted + mounts_deleted,
        "file_errors": wasm_errs
    }))
}


/// Deletes a single module by its id or name. Also removes all files related to it.
pub async fn delete_module_by_id(path: web::Path<String>) -> HttpResponse {
    let key = path.into_inner();
    let coll = get_collection::<Document>("module").await;

    // Get the module document
    let filter = module_filter(&key);
    let doc_opt = match coll.find_one(filter.clone()).await {
        Ok(d) => d,
        Err(e) => {
            error!("Lookup failed for '{}': {}", key, e);
            return HttpResponse::InternalServerError().json(json!({"message":"Lookup failed"}));
        }
    };

    // Return error if no module matched the query (id or name)
    let Some(doc) = doc_opt else {
        return HttpResponse::NotFound().json(json!({"message":"Module not found","query": key}));
    };

    // Get the modules _id
    let module_oid_hex = match doc.get_object_id("_id") {
        Ok(oid) => oid.to_hex(),
        Err(_) => {
            warn!("Module document missing valid _id, won't be able to delete module cards related to it.");
            String::new()
        }
    };

    // Delete related module card if _id was found
    if !module_oid_hex.is_empty() {
        let _ = delete_module_card_by_id(web::Path::<String>::from(module_oid_hex.clone())).await;
    }

    // Delete all files related to the module
    let wasm_path = doc.get_document("wasm")
        .ok()
        .and_then(|w| w.get_str("path").ok())
        .map(|s| s.to_string());

    let mut files_deleted = 0usize;
    let mut file_errors: Vec<String> = Vec::new();
    if let Some(path) = wasm_path {
        try_delete_file(&path, &mut files_deleted, &mut file_errors);
    }
    for p in collect_datafile_paths(&doc) {
        try_delete_file(&p, &mut files_deleted, &mut file_errors);
    }

    // Delete the module doc
    match coll.delete_one(filter).await {
        Ok(res) if res.deleted_count == 1 => HttpResponse::Ok().json(json!({
            "message":"Module deleted",
            "query": key,
            "files_deleted": files_deleted,
            "file_errors": file_errors
        })),
        Ok(_) => HttpResponse::NotFound().json(json!({"message":"Module not found during delete","query": key})),
        Err(e) => {
            error!("Failed to delete module doc '{}': {}", key, e);
            HttpResponse::InternalServerError().json(json!({
                "message":"Failed to delete module document",
                "query": key,
                "files_deleted": files_deleted,
                "file_errors": file_errors
            }))
        }
    }
}


/// Endpoint for getting all module docs from database
pub async fn get_all_modules() -> HttpResponse {
    let coll = get_collection::<Document>("module").await;
    let mut cursor = match coll.find(doc! {}).await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Error querying modules: {}", e);
            return HttpResponse::InternalServerError().json(json!({
                "message": "Error querying modules"
            }));
        }
    };
    let mut out: Vec<Document> = Vec::new();
    while let Some(mut doc) = cursor.try_next().await.unwrap_or(None) {
        if let Some(oid) = doc.get_object_id("_id").ok() {
            doc.insert("_id", Bson::String(oid.to_hex()));
        }
        out.push(doc);
    }
    HttpResponse::Ok().json(out)
}


/// Endpoint for getting one module doc by its name/id from database.
pub async fn get_module_by_id(path: web::Path<String>) -> HttpResponse {
    let id_str = path.into_inner();
    let coll = get_collection::<Document>("module").await;
    let filter = module_filter(&id_str);
    match coll.find_one(filter).await {
        Ok(Some(mut doc)) => {
            if let Ok(oid_ref) = doc.get_object_id("_id") {
                doc.insert("_id", Bson::String(oid_ref.to_hex()));
            }
            HttpResponse::Ok().json(vec![doc])
        }
        Ok(None) => HttpResponse::Ok().json(Vec::<Document>::new()), // []
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": "Error querying module",
            "error": e.to_string()
        })),
    }
}


/// Endpoint that takes the module description as an html form (multipart request), and
/// creates an openapi documentation for the related module from that. 
pub async fn describe_module(
    path: web::Path<String>,
    payload: Multipart,
) -> HttpResponse {

    // TODO: Find a better way to parse the description fields. The current one sucks to update.

    // Get the multipart summary first. This is helpful because handle_multipart_request
    // handles correctly saving files that came with the request.
    let summary = match handle_multipart_request(payload).await {
        Ok(s) => s,
        Err(e) => {
            error!("❌ multipart handling failed: {e}");
            return HttpResponse::InternalServerError().body("Failed to process multipart");
        }
    };

    // After handling the incoming multipart request, find the module that the mounts and description
    // that were sent with the request are related to. Fail miserably if the module is not found.
    let key = path.into_inner();
    let filter = module_filter(&key);
    let coll = get_collection::<Document>("module").await;
    let module_doc = match coll.find_one(filter.clone()).await {
        Ok(Some(d)) => d,
        Ok(None) => return HttpResponse::NotFound().body("Module not found"),
        Err(e) => {
            error!("Database error when searching for a module related to module description: {e}");
            return HttpResponse::InternalServerError().body("Database error");
        }
    };
    let module_name = module_doc.get_str("name").unwrap_or("unknown");

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
            return HttpResponse::BadRequest().json(json!({
                "message": "No description was provided, or description was malformed."
            }));
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
                return HttpResponse::BadRequest().json(json!({
                    "message": format!("Functions missing mounts: {}", serde_json::to_string(&actually_missing).unwrap_or_default())
                }));
            }
        } else {
            return HttpResponse::BadRequest().json(json!({
                "message": format!("Functions missing mounts: {}", serde_json::to_string(&missing).unwrap_or_default())
            }));
        }
    }

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
    let openapi_json = module_endpoint_descriptions(module_name, &functions);
    let description_doc: Document = bson::to_document(&openapi_json).unwrap_or_else(|_| Document::new());
    update_doc.insert("description", Bson::Document(description_doc));

    // Update the entry related to the current module with the openapi description, mount listing and datafile list.
    let update = doc! { "$set": update_doc };
    if let Err(e) = coll.update_many(filter, update).await {
        error!("Failed to update module with mounts/description: {e}");
        return HttpResponse::InternalServerError().body("update failed");
    }
    HttpResponse::Ok().json(json!({ "description": openapi_json }))
}


/// Creates an openapi descriptions from module name and a list of functions and their specs
pub fn module_endpoint_descriptions(
    module_name: &str,
    functions: &HashMap<String, FunctionSpec>,
) -> Value {

    // Create a map of paths, where each path has their own specifications
    // constructed from the 'functions' hashmap into expected openapi format.
    // This is done by iterating through each function, since each function will have 
    // their own path.
    let mut paths = Map::new();
    for (func_name, func) in functions {
        // Parameters element of a path object (same for every path)
        let params_element = vec![json!({
            "name": "deployment",
            "in": "path",
            "description": "Deployment ID",
            "required": true,
            "schema": { "type": "string" }
        })];

        // Function query parameters for the current function. Each parameter has their own entry.
        // These will be listed under the http method related key 
        // (for example {'path':'get':'parameters':[func_params]} )
        let func_params: Vec<Value> = func.parameters.iter().map(|p| {
            json!({
                "name": p.name,
                "in": "query",
                "description": "Auto-generated description",
                "required": true,
                "schema": { "type": p.ty }
            })
        }).collect();

        // 'responses' element under the path item. More specifically, this is also listed under
        // the http method related key under the path, for example
        // {'path':'get':'responses':{'200':{success_content}}}
        let success_content = if is_primitive(&func.output_type) {
            json!({
                "application/json": {
                    "schema": { "type": func.output_type }
                }
            })
        } else {
            json!({
                func.output_type.clone(): {
                    "schema": { "type": "string", "format": "binary" }
                }
            })
        };

        // The http method that this path should be called with. Stored as for example {'path':'get':{}}
        // Contains the elements 'tags', 'summary', 'parameters' and 'responses', some of which were constructed
        // earlier.
        let method_key = func.method.to_lowercase();
        let mut http_method_key = Map::new();
        http_method_key.insert("tags".into(), json!([]));
        http_method_key.insert("summary".into(), json!("Auto-generated description of function call method"));
        http_method_key.insert("parameters".into(), Value::Array(func_params));
        http_method_key.insert("responses".into(), json!({
            "200": {
                "description": "Auto-generated description of response",
                "content": success_content
            }
        }));

        // Optional element containing information on the input mounts of a function (that is a mount
        // other than an output mount). Only added if there are other mounts than output mounts
        let input_mounts: Vec<(&String, &MountSpec)> = func
            .mounts
            .iter()
            .filter(|(_name, m)| m.stage != "output")
            .collect();
        if !input_mounts.is_empty() {

            // Set the properties for each mount. They all are set to have type:string and format:binary.
            let mut properties = Map::new();
            for (name, _m) in &input_mounts {
                properties.insert(
                    (*name).clone(),
                    json!({ "type": "string", "format": "binary" }),
                );
            }
            // Set the encoding/content-type for each mount. Usually this will end up being 
            // application/octet-stream
            let mut encoding = Map::new();
            for (name, m) in &input_mounts {
                encoding.insert((*name).clone(), json!({ "contentType": m.media_type }));
            }

            // Build the final request_body object, and save it under the current functions http method key
            let request_body = json!({
                "required": true,
                "content": {
                    "multipart/form-data": {
                        "schema": { "type": "object", "properties": properties },
                        "encoding": encoding
                    }
                }
            });
            http_method_key.insert("requestBody".into(), request_body);
        }

        // Build the final path item, that is stored under a specific path under 'paths' key in openapi doc
        let mut path_item = Map::new();
        path_item.insert("summary".into(), json!("Auto-generated description of function"));
        path_item.insert("parameters".into(), Value::Array(params_element));
        path_item.insert(method_key, Value::Object(http_method_key));
        let path = supervisor_execution_path(module_name, func_name);
        paths.insert(path, Value::Object(path_item));
    }

    // Final openapi document format
    // TODO: Should this be turned into a struct?
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": module_name,
            "description": "Calling microservices defined as WebAssembly functions",
            "version": "0.0.1"
        },
        "tags": [
            { "name": "WebAssembly", "description": "Executing WebAssembly functions" }
        ],
        "servers": [
            {
                "url": "http://{serverIp}:{port}",
                "variables": {
                    "serverIp": {
                        "default": "localhost",
                        "description": "IP or name found with mDNS of the machine running supervisor"
                    },
                    "port": { "enum": ["5000","80"], "default": "5000" }
                }
            }
        ],
        "paths": Value::Object(paths)
    })
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


/// Endpoint for getting a modules description by its id/name
pub async fn get_module_description_by_id(path: web::Path<String>) -> HttpResponse {
    let id_str = path.into_inner();
    let coll = get_collection::<Document>("module").await;
    let filter = module_filter(&id_str);
    match coll.find_one(filter).await {
        Ok(Some(doc)) => {
            match doc.get_document("description") {
                Ok(desc) => HttpResponse::Ok().json(desc),
                Err(_)   => HttpResponse::Ok().json(Document::new()),
            }
        }
        Ok(None) => {
            HttpResponse::NotFound().json(json!({
                "message": "Module not found",
                "id": id_str
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": "Error querying module",
            "error": e.to_string()
        })),
    }
}


/// Endpoint that returns a given modules datafile/mounted file based on the given name.
/// The name must match the key for that file in the database, not the actual filename it has
/// in the filesystem. For module, accepts either modules id, or its name.
pub async fn get_module_datafile(
    _req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<NamedFile> {
    let (id_str, datafile_key) = path.into_inner();
    let coll = get_collection::<Document>("module").await;
    let filter = module_filter(&id_str);

    // Load module doc
    let doc = coll
        .find_one(filter)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Module not found"))?;

    // Get the datafiles section of module docs, if it exists.
    let df_doc = doc
        .get_document("dataFiles")
        .map_err(|_| actix_web::error::ErrorNotFound("No dataFiles for this module"))?;

    // Get the correct datafile information, if the given key matches any.
    let file_obj = df_doc
        .get_document(&datafile_key)
        .map_err(|_| actix_web::error::ErrorNotFound("Datafile key not found"))?;

    // Get the path to the datafile, if it exists in the filesystem.
    let path = file_obj
        .get_str("path")
        .map_err(|_| actix_web::error::ErrorInternalServerError("Datafile not found in the filesystem."))?;

    // Guess the mimetype of the file and return the file as response
    let mut named = NamedFile::open(path)
        .map_err(|_| actix_web::error::ErrorNotFound("File not found on disk"))?;
    let guessed = mime_guess::from_path(path)
        .first_or_octet_stream();
    named = named.set_content_type(guessed);
    Ok(named)
}