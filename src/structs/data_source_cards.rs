use serde::{Serialize, Deserialize};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasourceCard {
    #[serde(rename="_id", skip_serializing_if="Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "risk-level")]
    pub risk_level: String,
    pub nodeid: ObjectId,
    #[serde(rename="dateReceived", with = "chrono_datetime_as_bson_datetime")]
    pub date_received: DateTime<Utc>
}