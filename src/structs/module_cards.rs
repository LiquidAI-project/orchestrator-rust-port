use serde::{Serialize, Deserialize};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCard {
    #[serde(rename="_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    pub moduleid: ObjectId,
    pub name: String,
    #[serde(rename = "risk-level")]
    pub risk_level: String,
    #[serde(rename = "input-type")]
    pub input_type: String,
    #[serde(rename = "output-risk")]
    pub output_risk: String,
    #[serde(rename="dateReceived", with = "chrono_datetime_as_bson_datetime")]
    pub date_received: DateTime<Utc>
}