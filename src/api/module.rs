use crate::lib::constants::{MODULE_DIR, WASMIOT_INIT_FUNCTION_NAME};
use crate::lib::mongodb::insert_one;
use actix_web::{HttpResponse, web};
use serde_json::json;
use mongodb::bson::{self, Bson, doc, oid::ObjectId, Document};
use actix_multipart::Multipart;
use futures_util::stream::StreamExt;
use std::io::Write;
use log::{error, info, warn, debug};
use serde::{Serialize, Deserialize};
use wasmtime::{Engine, ExternType, Module, ValType};



// TODO: Module updates (and their notifications if they are already deployed)
// TODO: Module deletion, for one module and all modules (clear database and module folder)
// TODO: Adding module descriptions
// TODO: 3 different routes for getting module info (check js version)

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


#[derive(Debug, Serialize, Deserialize)]
pub struct DataFiles {
    // TODO: Represents data files, figure out what goes here (added in description phase)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenApiDescription {
    // TODO: The openapi description that is automatically generated (added in description phase)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mounts {
    // TODO: List of mounts (added in description phase)
}

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

/// This function is meant to handle multipart requests that might or might not
/// contain multiple files and fields. It processes the request body, extracts the
/// separate fields into json, and saves files to disk while adding saved file information
/// on the returned json as well.
async fn handle_multipart_request(mut payload: Multipart) -> Result<MultipartSummary, actix_web::Error> {
    // TODO: Likely doesnt handle nested fields well, if those exist in multipart requests.

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

        // Check that content disposition is form data, ignore others
        let content_disposition = field.content_disposition();
        if !(content_disposition.is_some_and(|cd| cd.is_form_data())) {
            warn!("⚠️ Ignoring non-form-data field in module upload: {:?}", content_disposition);
            continue;
        } else {
            warn!("⚠️ Failed to read content-disposition in module upload: {:?}", content_disposition);
        }

        // Extract field metadata
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

        // If field has no content type, assume its a plain text field
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

        // If the field has content type of application/wasm, treat it as a wasm file
        if mimetype == "application/wasm" {

            let mut uploaded  = UploadedFile {
                fieldname: String::new(),
                originalname: String::new(),
                filename: String::new(),
                path: String::new(),
                size: 0,
                mimetype: String::new()
            };

            let saved_name = format!("{}.wasm", uuid::Uuid::new_v4());
            let filepath = format!("{}/{}", MODULE_DIR, saved_name);

            let mut f = match std::fs::File::create(&filepath) {
                Ok(file) => file,
                Err(e) => {
                    error!("❌ Failed to create wasm file: {e}");
                    return Err(actix_web::error::ErrorInternalServerError("Failed to create wasm file to disk."));
                }
            };

            while let Some(Ok(chunk)) = field.next().await {
                if let Err(e) = f.write_all(&chunk) {
                    error!("❌ Failed to write wasm file: {e}");
                    return Err(actix_web::error::ErrorInternalServerError("Failed to write wasm file to disk."));
                }
            }
            let meta = std::fs::metadata(&filepath)?;
            debug!("📦 Saved .wasm file to disk: {}", filepath);
            uploaded.fieldname = name;
            uploaded.originalname = filename;
            uploaded.filename = saved_name;
            uploaded.path = filepath;
            uploaded.size = meta.len() as usize;
            uploaded.mimetype = mimetype;
            summary.files.push(uploaded);
            continue;
        }

        // TODO: Add image types and others to be saved as mounts here

        error!("❌ Unsupported mimetype '{}', ignoring...", mimetype);

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


/// Parses a WebAssembly module at the given path and returns its requirements and exports
/// as a tuple of vectors containing `WasmRequirement` and `WasmExport` structs.
fn parse_wasm_at_path(
    path: &str,
) -> Result<(Vec<WasmRequirement>, Vec<WasmExport>), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes)?; // compile only

    // Get the imports from the module
    let requirements: Vec<WasmRequirement> = module
        .imports()
        .filter_map(|imp| match imp.ty() {
            ExternType::Func(fty) => Some(WasmRequirement { // If the import is a function, convert it to a WasmRequirement
                module: imp.module().to_string(),
                name: imp.name().to_string(),
                kind: "function".to_string(),
                params: fty.params().map(valtype_to_string).collect(),
                results: fty.results().map(valtype_to_string).collect(),
            }),
            _ => None, // Disregard all non-function imports for now
        })
        .collect();

    // Get the exports from the module
    let exports: Vec<WasmExport> = module
        .exports()
        .filter_map(|ex| match ex.ty() {
            ExternType::Func(fty) => Some(WasmExport { // If the export is a function, convert it to a WasmExport
                name: ex.name().to_string(),
                parameter_count: fty.params().len(),
                params: fty.params().map(valtype_to_string).collect(),
                results: fty.results().map(valtype_to_string).collect(),
            }),
            _ => None, // Disregard all non-function exports for now
        })
        .collect();

    Ok((requirements, exports))
}


/// Converts a `ValType` to a string representation.
fn valtype_to_string(t: ValType) -> String {
    match t {
        ValType::I32 => "i32".to_string(),
        ValType::I64 => "i64".to_string(),
        ValType::F32 => "f32".to_string(),
        ValType::F64 => "f64".to_string(),
        ValType::V128 => "v128".to_string(),
        _ => format!("{:?}", t), // Wildcard for any other types
    }
}


