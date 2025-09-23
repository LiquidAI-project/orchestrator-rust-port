use std::{env, fs, io};
use std::path::{Path, PathBuf};
use chrono::Utc;
use log::{error, info, warn};
use mongodb::{bson::{doc, Bson, Document, oid::ObjectId, to_bson}, Collection};
use crate::lib::mongodb as db;
use crate::lib::zeroconf::get_listening_address;

const DEVICE: &str = "device";
const MODULE: &str = "module";
const DEPLOYMENT: &str = "deployment";
const FILES: &str = "files";


// TODO: Use the structs for devices/modules/deployments to validate their structure before adding to database


/// Call this function to start the process of adding data from the init folder to the database.
/// Init folder is read from the WASMIOT_INIT_FOLDER env var, or is assumed to be just "./init"
pub async fn add_initial_data() -> anyhow::Result<()> {
    let init_folder = env::var("WASMIOT_INIT_FOLDER").unwrap_or_else(|_| "./init".to_string());
    if !Path::new(&init_folder).exists() {
        info!("Init folder '{}' not found. Skipping DB bootstrap.", init_folder);
        return Ok(());
    }

    if !has_content(&init_folder)? {
        info!("No JSON found under '{}/{{device,module,deployment}}'. Skipping DB bootstrap.", init_folder);
        return Ok(());
    }

    info!("Starting DB bootstrap from '{}'...", init_folder);

    init_devices(&init_folder).await?;
    init_modules(&init_folder).await?;
    init_deployments(&init_folder).await?;

    let clear_logs = env::var("WASMIOT_CLEAR_LOGS").map(|v| v == "true").unwrap_or(false);
    if clear_logs {
        remove_supervisor_logs().await?;
    }

    info!("Bootstrap completed.");
    Ok(())
}


/// Returns true if there's at least one non-hidden, non-empty JSON file among device/module/deployment folders.
fn has_content(root: &str) -> io::Result<bool> {
    for sub in [DEVICE, MODULE, DEPLOYMENT] {
        let dir = Path::new(root).join(sub);
        if !dir.exists() { continue; }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') { continue; } // ignore .placeholder files
            if !name.ends_with(".json") { continue; }
            let meta = entry.metadata()?;
            if meta.len() > 0 { return Ok(true); }
        }
    }
    Ok(false)
}


/// Fills out the devices collection based on JSON documents found in the init/device folder.
async fn init_devices(root: &str) -> anyhow::Result<()> {
    let coll: Collection<Document> = db::get_collection(DEVICE).await;
    let mut devices = load_json_documents(Path::new(root).join(DEVICE))?;

    // Filter out orchestrator device by name
    devices.retain(|d| match d.get_str("name") { Ok(n) => n != "orchestrator", _ => true });

    // Set health timestamp to now if timestamp is found
    let now = Bson::DateTime(mongodb::bson::DateTime::from_chrono(Utc::now()));
    for d in &mut devices {
        if let Some(Bson::Document(health)) = d.get_mut("health") {
            if let Some(v) = health.get_mut("time_of_query") { *v = now.clone(); }
        }
        // Ensure _id is ObjectId if provided as string
        convert_string_to_objectid(d, "_id");
    }

    if devices.is_empty() {
        info!("No initial device data found. Leaving the database as is.");
        return Ok(());
    }

    info!("Clearing non-orchestrator devices from DB...");
    if let Err(e) = coll.delete_many(doc!{"name": {"$ne": "orchestrator"}}).await {
        error!("Failed to clear devices: {}", e);
    }

    info!("Inserting {} devices...", devices.len());
    if let Err(e) = coll.insert_many(devices).await { error!("Failed to add devices: {}", e); }
    Ok(())
}


/// Fills out the modules collection based on JSON documents found in the init/module folder.
async fn init_modules(root: &str) -> anyhow::Result<()> {
    let coll: Collection<Document> = db::get_collection(MODULE).await;
    let mut modules = load_json_documents(Path::new(root).join(MODULE))?;

    // Filter out core modules if they are there
    modules.retain(|m| match m.get("isCoreModule") { Some(Bson::Boolean(true)) => false, _ => true });

    let required_files = get_required_files(root, &modules);
    if modules.is_empty() {
        info!("No initial module data found. Leaving the database as is.");
        return Ok(());
    }
    for m in &mut modules { convert_string_to_objectid(m, "_id"); }

    info!("Clearing non-core modules from DB...");
    let filter = doc!{"$or": [{"isCoreModule": {"$exists": false}}, {"isCoreModule": {"$ne": true}}]};
    if let Err(e) = coll.delete_many(filter).await { error!("Failed to clear modules: {}", e); }

    info!("Inserting {} modules...", modules.len());
    if let Err(e) = coll.insert_many(modules).await { error!("Failed to add modules: {}", e); }

    info!("Copying required files...");
    copy_files(&required_files)?;
    Ok(())
}


/// Fills out the deployments collection based on JSON documents found in the init/deployment folder.
async fn init_deployments(root: &str) -> anyhow::Result<()> {
    let coll: Collection<Document> = db::get_collection(DEPLOYMENT).await;
    let mut deployments = load_json_documents(Path::new(root).join(DEPLOYMENT))?;

    if deployments.is_empty() {
        info!("No initial deployment data found. Leaving the database as is.");
        return Ok(());
    }

    for d in &mut deployments {
        convert_string_to_objectid(d, "_id");
        if let Some(Bson::Array(seq)) = d.get_mut("sequence") {
            for item in seq {
                if let Bson::Document(step) = item {
                    convert_string_to_objectid(step, "device");
                    convert_string_to_objectid(step, "module");
                }
            }
        }

        if let Some(Bson::Document(full)) = d.get_mut("fullManifest") {
            for (_, device_doc) in full.iter_mut() {
                if let Bson::Document(dev) = device_doc {
                    convert_string_to_objectid(dev, "deploymentId");
                    if let Some(Bson::Array(mods)) = dev.get_mut("modules") {
                        for m in mods.iter_mut() {
                            if let Bson::Document(mdoc) = m {
                                convert_string_to_objectid(mdoc, "id");
                                if let Some(Bson::Document(urls)) = mdoc.get_mut("urls") {
                                    rewrite_module_urls(urls);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove active flag if present, it is assumed that none of the just read deployments is already active anywhere.
        d.remove("active");
    }

    info!("Clearing deployments from DB...");
    if let Err(e) = coll.delete_many(doc!{}).await { error!("Failed to clear deployments: {}", e); }
    info!("Inserting {} deployments...", deployments.len());
    if let Err(e) = coll.insert_many(deployments).await { error!("Failed to add deployments: {}", e); }
    Ok(())
}


/// Clears the supervisorLogs collection.
async fn remove_supervisor_logs() -> anyhow::Result<()> {
    let coll: Collection<Document> = db::get_collection("supervisorLogs").await;
    info!("Clearing supervisor logs...");
    if let Err(e) = coll.delete_many(doc!{}).await { error!("Failed to clear logs: {}", e); }
    Ok(())
}


/// Read all .json files from a given directory. Returns a vector of JSON documents, if any are found.
fn load_json_documents(dir: PathBuf) -> anyhow::Result<Vec<Document>> {
    let mut out: Vec<Document> = Vec::new();
    if !dir.exists() { return Ok(out); }

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') { continue; }
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }

        let raw = fs::read_to_string(&path)?;
        let val: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => { warn!("Failed to parse JSON in {:?}: {}", path, e); continue; }
        };

        match val {
            serde_json::Value::Array(arr) => {
                for v in arr { if let Ok(doc) = json_to_document(v) { out.push(doc) } else { warn!("Invalid JSON object in {:?}", path); } }
            }
            serde_json::Value::Object(_) => {
                if let Ok(doc) = json_to_document(val) { out.push(doc); } else { warn!("Invalid JSON object in {:?}", path); }
            }
            _ => warn!("Ignoring non-object JSON in {:?}", path)
        }
    }
    Ok(out)
}


/// Helper function that converts a serde_json::Value into a mongodb Document
fn json_to_document(v: serde_json::Value) -> anyhow::Result<Document> {
    let bson = to_bson(&v)?;
    let mut doc = bson.as_document().cloned().ok_or_else(|| anyhow::anyhow!("expected JSON object"))?;
    convert_string_to_objectid(&mut doc, "_id");
    Ok(doc)
}


/// Helper function that converts a string into a mongodb ObjectId
fn convert_string_to_objectid(doc: &mut Document, key: &str) {
    if let Some(Bson::String(s)) = doc.get(key) {
        if let Ok(oid) = ObjectId::parse_str(s) { doc.insert(key, Bson::ObjectId(oid)); }
    }
}


/// Helper function that collects a list of all required wasm files and datafiles for a module document.
/// Returns a vector of tuples, where the first item is the source path and the second item is the destination path.
fn get_required_files(root: &str, modules: &[Document]) -> Vec<(PathBuf, PathBuf)> {
    let mut files: Vec<(PathBuf, PathBuf)> = Vec::new();
    let files_dir = Path::new(root).join(FILES);

    for m in modules {
        if let Some(Bson::Document(wasm)) = m.get("wasm") {
            let src = wasm.get_str("originalFilename").ok().map(|f| files_dir.join(f));
            let dst = wasm.get_str("path").ok().map(|p| PathBuf::from(p));
            if let (Some(s), Some(d)) = (src, dst) { files.push((s, d)); }
        }
        if let Some(Bson::Document(df)) = m.get("dataFiles") {
            for (_, v) in df {
                if let Bson::Document(one) = v {
                    let src = one.get_str("originalFilename").ok().map(|f| files_dir.join(f));
                    let dst = one.get_str("path").ok().map(|p| PathBuf::from(p));
                    if let (Some(s), Some(d)) = (src, dst) { files.push((s, d)); }
                }
            }
        }
    }
    files
}


/// Helper function for copying files. Takes a list of tuples containing a source path
/// and a destination path.
fn copy_files(files: &[(PathBuf, PathBuf)]) -> anyhow::Result<()> {
    let mut copied = 0usize;
    for (src, dst) in files {
        if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
        match fs::copy(src, dst) {
            Ok(_) => { copied += 1; }
            Err(e) => warn!("Failed to copy {:?} -> {:?}: {}", src, dst, e)
        }
    }
    info!("Copied {} files.", copied);
    Ok(())
}


/// Helper function to get the listening address for the orchestrator
fn get_orchestrator_address() -> String {
    let (orchestrator_host, orchestrator_port) = get_listening_address();
    let package_manager_base_url = std::env::var("PACKAGE_MANAGER_BASE_URL")
            .unwrap_or_else(|_| format!("http://{}:{}", orchestrator_host, orchestrator_port));
    return package_manager_base_url;
}


/// Helper function to rewrite some of the module urls
fn rewrite_module_urls(urls: &mut Document) {
    let base = get_orchestrator_address();
    for key in ["binary", "description"] {
        if let Some(Bson::String(u)) = urls.get_mut(key) {
            *u = replace_public_base_uri(u, &base);
        }
    }
    if let Some(Bson::Document(other)) = urls.get_mut("other") {
        for (_, v) in other.iter_mut() {
            if let Bson::String(u) = v { *u = replace_public_base_uri(u, &base); }
        }
    }
}


/// Helper function to replace the orchestrator address
fn replace_public_base_uri(url: &str, internal_base: &str) -> String {
    const SPLITTER: &str = "file/module";
    if let Some(idx) = url.find(SPLITTER) {
        let tail = &url[idx..];
        format!("{}{}", internal_base, tail)
    } else {
        url.to_string()
    }
}

