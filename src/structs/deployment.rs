use crate::structs::module::MountStage;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;
use crate::structs::openapi::{
    OpenApiEncodingObject, 
    OpenApiSchemaObject, 
    OpenApiParameterObject
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentDoc {
    #[serde(rename = "_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub sequence: Vec<SequenceStep>,
    #[serde(rename = "validationError", skip_serializing_if="Option::is_none")]
    pub validation_error: Option<String>,
    #[serde(rename = "fullManifest")]
    pub full_manifest: HashMap<String, DeploymentNode>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub active: Option<bool>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceStep {
    pub device: ObjectId,
    pub module: ObjectId,
    pub func: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentNode {
    #[serde(rename="deploymentId")]
    pub deployment_id: ObjectId,
    pub modules: Vec<DeviceModule>,
    pub endpoints: HashMap<String, HashMap<String, Endpoint>>,
    pub instructions: Instructions,
    pub mounts: HashMap<String, HashMap<String, StageMounts>>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub media_type: String,
    #[serde(default)]
    pub schema: Option<OpenApiSchemaObject>,
    #[serde(default)] 
    pub encoding: Option<HashMap<String, OpenApiEncodingObject>>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRequest {
    #[serde(default)]
    pub parameters: Vec<OpenApiParameterObject>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub request_body: Option<RequestBody>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResponse {
    pub media_type: String,
    #[serde(default)]
    pub schema: Option<OpenApiSchemaObject>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub url: String,
    pub path: String,
    pub method: String,
    pub request: OperationRequest,
    pub response: OperationResponse,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceModuleUrls {
    pub binary: String,
    pub description: String,
    pub other: HashMap<String, String>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceModule {
    pub id: ObjectId,
    pub name: String,
    pub urls: DeviceModuleUrls,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub from: Endpoint,
    pub to: Option<Endpoint>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instructions {
    pub modules: HashMap<String, HashMap<String, Instruction>>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartMediaType {
    pub media_type: String,
    pub schema: SchemaObject,
    pub encoding: HashMap<String, OpenApiEncodingObject>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaObject {
    #[serde(rename="type")]
    pub r#type: String,
    #[serde(default)]
    pub properties: HashMap<String, SchemaProperty>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaProperty {
    #[serde(rename="type")]
    pub r#type: String,
    #[serde(default)]
    pub format: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPathFile {
    pub path: String,
    pub media_type: String,
    #[serde(default)]
    pub stage: Option<MountStage>
}
impl MountPathFile {

    /// Does validation and collects mounts into MountPathFiles
    pub fn list_from_multipart(m_obj: &MultipartMediaType) -> Result<Vec<MountPathFile>, String> {

        // Validation
        if m_obj.media_type != "multipart/form-data" {
            return Err(format!("Expected multipart/form-data, got '{:?}'", m_obj.media_type));
        }
        if m_obj.schema.r#type != "object" {
            return Err(format!("Only object schemas supported, got '{:?}'", m_obj.schema.r#type));
        }
        if m_obj.schema.properties.is_empty() {
            return Err(format!("Expected properties for multipart schema, properties was empty instead."));
        }

        // Collect mounts
        let mut mounts: Vec<MountPathFile> = Vec::new();
        for (path, property) in &m_obj.schema.properties {
            let is_binary =  property.r#type == "string" && matches!(property.format.as_deref(), Some("binary"));
            if !is_binary {
                continue;
            }
            if let Some(encoding) = m_obj.encoding.get(path) {
                if let Some(content_type) = encoding.content_type.as_deref() {
                    mounts.push(MountPathFile {
                        path: path.clone(),
                        media_type: content_type.to_string(),
                        stage: None
                    });
                }
            }
        }
        Ok(mounts)
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageMounts {
    #[serde(default)]
    pub execution: Vec<MountPathFile>,
    #[serde(default)]
    pub deployment: Vec<MountPathFile>,
    #[serde(default)]
    pub output: Vec<MountPathFile>,
}


