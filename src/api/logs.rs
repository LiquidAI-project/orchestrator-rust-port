use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use mongodb::bson::{self, doc, Document};
use actix_web::{web, HttpResponse, Responder};
use crate::lib::mongodb::{get_collection};
use futures::stream::TryStreamExt;
use actix_web::web::Form;

/// Struct to verify received log data structure from supervisor
#[derive(Debug, Serialize, Deserialize)]
pub struct LogData {
    #[serde(rename = "deviceIP")]
    pub device_ip: String,
    #[serde(rename = "deviceName")]
    pub device_name: String,
    #[serde(rename = "funcName")]
    pub func_name: String,
    #[serde(rename = "loglevel")]
    pub log_level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
    pub timestamp: String, // Timestamp of when the log was created and sent from the supervisor
}

/// Structure for the supervisor log data, this is the format its saved into database as
#[derive(Debug, Serialize, Deserialize)]
pub struct SupervisorLog {
    #[serde(rename = "deviceIP")]
    pub device_ip: String,
    #[serde(rename = "deviceName")]
    pub device_name: String,
    #[serde(rename = "funcName")]
    pub func_name: String,
    #[serde(rename = "loglevel")]
    pub log_level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "dateReceived", with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub date_received: DateTime<Utc>, // Timestamp of when this log was received by the orchestrator
}


/// Endpoint to receive and save supervisor logs
pub async fn post_supervisor_log(form: Form<std::collections::HashMap<String, String>>) -> impl Responder {
    if let Some(log_data_str) = form.get("logData") {
        let log_data: Value = match serde_json::from_str(log_data_str) {
            Ok(val) => val,
            Err(e) => {
                log::error!("Failed to parse logData as JSON: {}", e);
                return HttpResponse::BadRequest().body("Invalid logData JSON");
            }
        };
        log::debug!("Received supervisor log: {:?}", log_data);

        // Verify the log data structure
        let verified_supervisor_log: LogData = match serde_json::from_value::<LogData>(log_data.clone()) {
            Ok(log) => log, 
            Err(e) => {
                log::error!("Failed to convert log_data to SupervisorLog: \n{}\nReceived supervisor log: {:?}", e, log_data.clone());
                return HttpResponse::BadRequest().body("Invalid logData structure");
            }
        };

        // Convert the timestamp in log data into datetime
        let timestamp_str = log_data.get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let timestamp = match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                log::error!("Failed to parse timestamp: {}", e);
                return HttpResponse::BadRequest().body("Invalid timestamp format in logData");
            }
        };

        // Save the log data in the database in correct format
        let supervisor_log = SupervisorLog {
            device_ip: verified_supervisor_log.device_ip,
            device_name: verified_supervisor_log.device_name,
            func_name: verified_supervisor_log.func_name,
            log_level: verified_supervisor_log.log_level,
            message: verified_supervisor_log.message,
            request_id: verified_supervisor_log.request_id,
            deployment_id: verified_supervisor_log.deployment_id,
            module_name: verified_supervisor_log.module_name,
            timestamp,
            date_received: Utc::now(),
        };
        let doc: Document = bson::to_document(&supervisor_log).unwrap();
        let collection = get_collection::<Document>("supervisorLogs").await;
        match collection.insert_one(doc).await {
            Ok(_) => HttpResponse::Ok().json(json!({ "message": "Log received and saved" })),
            Err(e) => {
                log::error!("❌ Failed to insert supervisor log: {}", e);
                HttpResponse::InternalServerError().body("Log not saved")
            }
        }
    } else {
        HttpResponse::BadRequest().body("Missing logData field")
    }
}

/// Endpoint to retrieve supervisor logs with optional filtering 
pub async fn get_supervisor_logs(query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {

    // Optional time filter
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        if let Ok(dt) = DateTime::parse_from_rfc3339(after) {
            let dt_utc = dt.with_timezone(&Utc);
            filter = doc! { "dateReceived": { "$gt": mongodb::bson::DateTime::from_chrono(dt_utc) } };
        }
    }

    let collection = get_collection::<Document>("supervisorLogs").await;

    match collection.find(filter).await {
        Ok(cursor) => {
            let logs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
            HttpResponse::Ok().json(logs)
        }
        Err(e) => {
            log::error!("❌ Failed to fetch supervisor logs: {}", e);
            HttpResponse::InternalServerError().body("Failed to fetch logs")
        }
    }
}

