use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationLog {
    pub device: String,
    pub module: String,
    pub func: String,
    pub node_zone: String,
    pub module_risk: String,
    pub input_risk: String,
    pub output_risk: String,
    pub valid: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCertificate {
    #[serde(rename="_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(with = "chrono_datetime_as_bson_datetime")]
    pub date: DateTime<Utc>,
    #[serde(rename = "deploymentId")]
    pub deployment_id: ObjectId,
    pub valid: bool,
    #[serde(rename = "validationLogs")]
    pub validation_logs: Vec<ValidationLog>,
}
