use std::{env, fs, io};
use std::path::{Path, PathBuf};
use log::{error, info, warn};
use mongodb::{bson::doc, Collection};
use futures::TryStreamExt;
use crate::lib::mongodb as db;
use crate::structs::logs::SupervisorLog;
use actix_web::{HttpResponse, Responder};

use crate::structs::data_source_cards::DatasourceCard;
use crate::structs::deployment_certificates::DeploymentCertificate;
use crate::structs::deployment::DeploymentDoc;
use crate::structs::device::DeviceDoc;
use crate::structs::module_cards::ModuleCard;
use crate::structs::module::ModuleDoc;
use crate::structs::node_cards::NodeCard;
use crate::structs::zones::Zones;
use crate::lib::errors::ApiError;

use crate::lib::constants::{ 
    COLL_DATASOURCE_CARDS, COLL_DEPLOYMENT, COLL_DEPLOYMENT_CERTS, COLL_DEVICE, COLL_LOGS, COLL_MODULE, COLL_MODULE_CARDS, COLL_NODE_CARDS, COLL_ZONES, FILE_ROOT_DIR
};


/// This function will save the current orchestrators entire setup into the ./init folder.
/// Will export all other database collections except for logs. Will also save the contents of
/// the ./files folder into ./init/files
/// 
/// The saved ./init folder can then be used to initialize orchestrator exactly as it was when
/// it was exported. Note that this doesnt mean it would also initialize supervisors as they
/// were, so if you want to export an entire orchestrator/supervisor setup, then you need 
/// to also create a docker compose file to maintain consistent enviroment.
pub async fn export_orchestrator_setup() -> anyhow::Result<()> {
    
    let datasourcecard_collection = db::get_collection::<DatasourceCard>(COLL_DATASOURCE_CARDS).await;
    let deployment_certificate_collection = db::get_collection::<DeploymentCertificate>(COLL_DEPLOYMENT_CERTS).await;
    let deployment_collection = db::get_collection::<DeploymentDoc>(COLL_DEPLOYMENT).await;
    let device_collection = db::get_collection::<DeviceDoc>(COLL_DEVICE).await;
    let modulecard_collection = db::get_collection::<ModuleCard>(COLL_MODULE_CARDS).await;
    let module_collection = db::get_collection::<ModuleDoc>(COLL_MODULE).await;
    let node_cards_collection = db::get_collection::<NodeCard>(COLL_NODE_CARDS).await;
    let zones_and_risk_levels_collection = db::get_collection::<Zones>(COLL_ZONES).await;

    // Recreate init folder to clear it out
    let init_folder = env::var("WASMIOT_INIT_FOLDER").unwrap_or_else(|_| "./init".to_string());
    delete_folder_contents(&init_folder)?;
    create_folder(&init_folder)?;

    // Copy the ./files folder content into new ./init folder
    copy_dir_into(FILE_ROOT_DIR, &init_folder)?;

    // Collect datasource cards and save them
    let _datasourcecards = datasourcecard_collection.find(doc! {}).await?;
    let datasourcecards: Vec<DatasourceCard> = _datasourcecards.try_collect().await?;
    let datasourcecards_folder_path = format!("{}/{}", init_folder, COLL_DATASOURCE_CARDS);
    create_folder(&datasourcecards_folder_path)?;
    for card in &datasourcecards {
        let Some(oid) = card.id.as_ref() else {
            warn!("Skipping exporting a datasourcecard without _id:\n{:?}", card);
            continue;
        };
        let file_path = PathBuf::from(&datasourcecards_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&card)?;
        fs::write(&file_path, json)?;
    }

    // Collect deployment certificates and save them
    let _deploymentcertificates = deployment_certificate_collection.find(doc! {}).await?;
    let deploymentcertificates: Vec<DeploymentCertificate> = _deploymentcertificates.try_collect().await?;
    let deploymentcertificates_folder_path = format!("{}/{}", init_folder, COLL_DEPLOYMENT_CERTS);
    create_folder(&deploymentcertificates_folder_path)?;
    for cert in &deploymentcertificates {
        let Some(oid) = cert.id.as_ref() else {
            warn!("Skipping exporting a deploymentcertificate without _id:\n{:?}", cert);
            continue;
        };
        let file_path = PathBuf::from(&deploymentcertificates_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&cert)?;
        fs::write(&file_path, json)?;
    }

    // Collect deployments and save them
    let _deployments = deployment_collection.find(doc! {}).await?;
    let deployments: Vec<DeploymentDoc> = _deployments.try_collect().await?;
    let deployments_folder_path = format!("{}/{}", init_folder, COLL_DEPLOYMENT);
    create_folder(&deployments_folder_path)?;
    for deployment in &deployments {
        let Some(oid) = deployment.id.as_ref() else {
            warn!("Skipping exporting a deployment without _id:\n{:?}", deployment);
            continue;
        };
        let file_path = PathBuf::from(&deployments_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&deployment)?;
        fs::write(&file_path, json)?;
    }

    // Collect devices and save them
    let _devices = device_collection.find(doc! {}).await?;
    let devices: Vec<DeviceDoc> = _devices.try_collect().await?;
    let devices_folder_path = format!("{}/{}", init_folder, COLL_DEVICE);
    create_folder(&devices_folder_path)?;
    for device in &devices {
        let Some(oid) = device.id.as_ref() else {
            warn!("Skipping exporting a device without _id:\n{:?}", device);
            continue;
        };
        let file_path = PathBuf::from(&devices_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&device)?;
        fs::write(&file_path, json)?;
    }

    // Collect module cards and save them
    let _modulecards = modulecard_collection.find(doc! {}).await?;
    let modulecards: Vec<ModuleCard> = _modulecards.try_collect().await?;
    let modulecards_folder_path = format!("{}/{}", init_folder, COLL_MODULE_CARDS);
    create_folder(&modulecards_folder_path)?;
    for card in &modulecards {
        let Some(oid) = card.id.as_ref() else {
            warn!("Skipping exporting a modulecard without _id:\n{:?}", card);
            continue;
        };
        let file_path = PathBuf::from(&modulecards_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&card)?;
        fs::write(&file_path, json)?;
    }

    // Collect modules and save them
    let _modules = module_collection.find(doc! {}).await?;
    let modules: Vec<ModuleDoc> = _modules.try_collect().await?;
    let modules_folder_path = format!("{}/{}", init_folder, COLL_MODULE);
    create_folder(&modules_folder_path)?;
    for module in &modules {
        let Some(oid) = module.id.as_ref() else {
            warn!("Skipping exporting a module without _id:\n{:?}", module);
            continue;
        };
        let file_path = PathBuf::from(&modules_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&module)?;
        fs::write(&file_path, json)?;
    }

    // Collect node cards and save them
    let _nodecards = node_cards_collection.find(doc! {}).await?;
    let nodecards: Vec<NodeCard> = _nodecards.try_collect().await?;
    let nodecards_folder_path = format!("{}/{}", init_folder, COLL_NODE_CARDS);
    create_folder(&nodecards_folder_path)?;
    for card in &nodecards {
        let Some(oid) = card.id.as_ref() else {
            warn!("Skipping exporting a nodecard without _id:\n{:?}", card);
            continue;
        };
        let file_path = PathBuf::from(&nodecards_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&card)?;
        fs::write(&file_path, json)?;
    }

    // Collect zones and risk levels and save them
    let _zones = zones_and_risk_levels_collection.find(doc! {}).await?;
    let zones: Vec<Zones> = _zones.try_collect().await?;
    let zones_folder_path = format!("{}/{}", init_folder, COLL_ZONES);
    create_folder(&zones_folder_path)?;
    for zone in &zones {
        let Some(oid) = zone.id.as_ref() else {//
            warn!("Skipping exporting a zone without _id:\n{:?}", zone);
            continue;
        };
        let file_path = PathBuf::from(&zones_folder_path).join(format!("{}.json", oid.to_hex()));
        let json = serde_json::to_string_pretty(&zone)?;
        fs::write(&file_path, json)?;
    }

    Ok(())

}


/// Endpoint for triggering orchestrator setup export
pub async fn handle_orchestrator_export() -> Result<impl Responder, ApiError> {
    if let Err(e) = export_orchestrator_setup().await {
        error!("Failed to export orchestrator setup: {}", e);
        return Err(ApiError::internal_error(format!("Failed to export orchestrator setup: {}", e)));
    }
    info!("Orchestrator setup exported successfully.");
    Ok(HttpResponse::Ok().finish())
}


/// Endpoint for triggering orchestrator setup import
pub async fn handle_orchestrator_import() -> Result<impl Responder, ApiError> {
    if let Err(e) = add_initial_data().await {
        error!("Failed to import orchestrator setup from init folder. Error: {:?}", e);
        Err(ApiError::internal_error(format!("Failed to import orchestrator setup from init folder, check logs for details")))
    } else {
        info!("Orchestrator setup successfully imported");
        Ok(HttpResponse::Ok().finish())
    }
}


/// This function imports an exported orchestrator setup from ./init/*
/// - Clears existing collections (and logs) from database
/// - Replaces ./files with ./init/files (if present)
/// - Imports each saved collection to database
pub async fn add_initial_data() -> anyhow::Result<()> {
    let init_folder = env::var("WASMIOT_INIT_FOLDER").unwrap_or_else(|_| "./init".to_string());
    let init_path = Path::new(&init_folder);

    if !init_path.exists() {
        info!("Init folder '{}' not found. Skipping import.", init_folder);
        return Ok(());
    }

    info!("Starting import from '{}' ...", init_folder);

    // 1) Replace ./files with ./init/files (if exists)
    let init_files = init_path.join("files");
    if init_files.exists() {
        if let Err(e) = delete_folder_contents(FILE_ROOT_DIR) {
            warn!("Failed to delete local files folder {:?}: {}", FILE_ROOT_DIR, e);
        }
        copy_dir_into(&init_files, ".")?;
        info!("Replaced '{}' from snapshot.", FILE_ROOT_DIR);
    } else {
        info!("No '{}/files' found in snapshot. Skipping files copy.", init_folder);
    }

    // 2) Clear collections (including logs)
    clear_collection::<DatasourceCard>(COLL_DATASOURCE_CARDS).await;
    clear_collection::<DeploymentCertificate>(COLL_DEPLOYMENT_CERTS).await;
    clear_collection::<DeploymentDoc>(COLL_DEPLOYMENT).await;
    clear_collection::<DeviceDoc>(COLL_DEVICE).await;
    clear_collection::<ModuleCard>(COLL_MODULE_CARDS).await;
    clear_collection::<ModuleDoc>(COLL_MODULE).await;
    clear_collection::<NodeCard>(COLL_NODE_CARDS).await;
    clear_collection::<Zones>(COLL_ZONES).await;
    clear_collection::<SupervisorLog>(COLL_LOGS).await;

    // 3) Import each collection from ./init/<collection>/*.json
    import_folder::<DatasourceCard>(init_path.join(COLL_DATASOURCE_CARDS), COLL_DATASOURCE_CARDS).await?;
    import_folder::<DeploymentCertificate>(init_path.join(COLL_DEPLOYMENT_CERTS), COLL_DEPLOYMENT_CERTS).await?;
    import_folder::<DeploymentDoc>(init_path.join(COLL_DEPLOYMENT), COLL_DEPLOYMENT).await?;
    import_folder::<DeviceDoc>(init_path.join(COLL_DEVICE), COLL_DEVICE).await?;
    import_folder::<ModuleCard>(init_path.join(COLL_MODULE_CARDS), COLL_MODULE_CARDS).await?;
    import_folder::<ModuleDoc>(init_path.join(COLL_MODULE), COLL_MODULE).await?;
    import_folder::<NodeCard>(init_path.join(COLL_NODE_CARDS), COLL_NODE_CARDS).await?;
    import_folder::<Zones>(init_path.join(COLL_ZONES), COLL_ZONES).await?;

    info!("Import completed.");
    Ok(())
}


/// Deletes *all* docs from a collection. 
async fn clear_collection<T: serde::de::DeserializeOwned + Unpin + Send + Sync>(name: &str) {
    let coll: Collection<T> = db::get_collection(name).await;
    if let Err(e) = coll.delete_many(doc!{}).await {
        error!("Failed to clear collection '{}': {}", name, e);
    } else {
        info!("Cleared collection '{}'", name);
    }
}


/// Helper function that imports typed entities from a folder of JSON files.
/// - Skips hidden files and non-JSON
/// - Skips files that fail to parse as the target struct
/// - Requires `_id` to be present in the JSON
async fn import_folder<T>(folder: PathBuf, coll_name: &str) -> anyhow::Result<()>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Unpin + Send + Sync + std::fmt::Debug,
{
    let coll: Collection<T> = db::get_collection(coll_name).await;

    if !folder.exists() {
        info!("No '{}' folder in snapshot. Skipping.", coll_name);
        return Ok(());
    }

    let mut ok_count = 0usize;
    let mut skip_count = 0usize;

    for entry in fs::read_dir(&folder)? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => { warn!("Failed to read entry in {:?}: {}", folder, e); continue; }
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if name.starts_with('.') { continue; }
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }

        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => { warn!("Failed to read {:?}: {}", path, e); skip_count += 1; continue; }
        };

        let parsed: T = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                warn!("File {:?} is not a valid {}: {}", path, coll_name, e);
                skip_count += 1; continue;
            }
        };

        let mut as_doc = match mongodb::bson::to_document(&parsed) {
            Ok(d) => d,
            Err(e) => { warn!("Failed to convert {:?} to BSON doc: {}", path, e); skip_count += 1; continue; }
        };

        // Check that id is present and convert to ObjectId if needed
        ensure_object_id(&mut as_doc);

        // Re-hydrate to T with normalized _id so type still matches collection
        let typed: T = match mongodb::bson::from_document::<T>(as_doc) {
            Ok(t) => t,
            Err(e) => { warn!("Failed to rehydrate {:?} into typed {}: {}", path, coll_name, e); skip_count += 1; continue; }
        };

        // Insert with id present so resulting id will be same as it was when exported
        match coll.insert_one(typed).await {
            Ok(_) => ok_count += 1,
            Err(e) => { warn!("Insert failed for {:?} into '{}': {}", path, coll_name, e); skip_count += 1; }
        }
    }

    info!("Imported {} '{}' docs (skipped {}).", ok_count, coll_name, skip_count);
    Ok(())
}


/// If document has a string `_id`, convert to `ObjectId`. If missing, ignore.
fn ensure_object_id(doc: &mut mongodb::bson::Document) {
    use mongodb::bson::{Bson, oid::ObjectId};

    match doc.get("_id") {
        Some(Bson::ObjectId(_)) => {} // fine
        Some(Bson::String(s)) => {
            if let Ok(oid) = ObjectId::parse_str(s) {
                doc.insert("_id", Bson::ObjectId(oid));
            }
        }
        _ => {
            // no _id
        }
    }
}



/// Helper function for deleting a folders contents
fn delete_folder_contents(path: &str) -> std::io::Result<()> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&p)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            fs::remove_dir_all(&p)?;
        } else {
            fs::remove_file(&p)?;
        }
    }
    Ok(())
}


/// Helper function for creating a folder
fn create_folder(path: &str) -> std::io::Result<()> {
    let p = Path::new(path);
    fs::create_dir_all(p)?;
    Ok(())
}


/// Helper function that copies a source folder into a specified destination folder.
/// Note that the destination folder will be the parent folder of the copied folder.
fn copy_dir_into(src_dir: impl AsRef<Path>, dst_parent: impl AsRef<Path>) -> io::Result<()> {
    let src_dir = src_dir.as_ref();
    let dst_dir = dst_parent.as_ref().join(
        src_dir.file_name().ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            "source path must be a directory with a valid name"
        ))?
    );

    copy_dir_recursive(src_dir, &dst_dir)
}


/// Helper function for the copying process. Recursively copies files to target directory,
/// recursing when encountering a folder.
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Source is not a directory"));
    }
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        } else {
            //
        }
    }
    Ok(())
}
