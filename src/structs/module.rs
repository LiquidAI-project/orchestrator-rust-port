use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;
use crate::structs::openapi::OpenApiDocument;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmExport {
    pub name: String,
    #[serde(rename = "parameterCount")]
    pub parameter_count: usize,
    pub params: Vec<String>, // List of function parameter types as strings
    pub results: Vec<String>, // List of function types as strings
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmRequirement {
    pub module: String,
    pub name: String,
    pub kind: String,
    pub params: Vec<String>, // List of function parameter types as strings
    pub results: Vec<String>, // List of function result types as strings
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmBinaryInfo {
    #[serde(rename = "originalFilename")]
    pub original_filename: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFileInfo {
    #[serde(rename = "originalFilename")]
    pub original_filename: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MountStage {
    Deployment,
    Execution,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleMount {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub stage: MountStage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDoc {
    #[serde(rename = "_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub exports: Vec<WasmExport>,
    pub requirements: Vec<WasmRequirement>,
    pub wasm: WasmBinaryInfo,
    #[serde(rename = "dataFiles", default, skip_serializing_if="Option::is_none")]
    pub data_files: Option<HashMap<String, DataFileInfo>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<OpenApiDocument>,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub mounts: Option<HashMap<String, HashMap<String, ModuleMount>>>,
    pub is_core_module: bool,
}