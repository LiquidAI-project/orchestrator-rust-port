use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use mongodb::bson::{self, doc, Document};
use actix_web::{web, HttpResponse, Responder};
use crate::lib::mongodb::{get_collection};
use futures::stream::TryStreamExt;
use actix_web::web::Form;
use crate::structs::logs::SupervisorLog;
use crate::lib::errors::ApiError;
use log::{debug, error};
use crate::lib::constants::COLL_LOGS;


/// Struct to verify received log data structure from supervisor.
/// Note that this is not the exact format the logs get saved into database as,
/// see SupervisorLog struct for that.
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


/// POST /device/logs
/// 
/// Endpoint to receive and save supervisor logs
pub async fn post_supervisor_log(form: Form<std::collections::HashMap<String, String>>) -> Result<impl Responder, ApiError> {
    if let Some(log_data_str) = form.get("logData") {
        let log_data: Value = match serde_json::from_str(log_data_str) {
            Ok(val) => val,
            Err(e) => {
                error!("Failed to parse logData as JSON: {}", e);
                return Err(ApiError::bad_request("Invalid logData JSON"));
            }
        };
        debug!("Received supervisor log: {:?}", log_data);

        // Verify the log data structure
        let verified_supervisor_log: LogData = match serde_json::from_value::<LogData>(log_data.clone()) {
            Ok(log) => log, 
            Err(e) => {
                error!("Failed to convert log_data to SupervisorLog: \n{}\nReceived supervisor log: {:?}", e, log_data.clone());
                return Err(ApiError::bad_request("Invalid logData structure"));
            }
        };

        // Convert the timestamp in log data into datetime
        let timestamp_str = log_data.get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let timestamp = match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                error!("Failed to parse timestamp: {}", e);
                return Err(ApiError::bad_request("Invalid timestamp format in logData"));
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
        let collection = get_collection::<Document>(COLL_LOGS).await;
        match collection.insert_one(doc).await {
            Ok(_) => Ok(HttpResponse::Ok().json(json!({ "message": "Log received and saved" }))),
            Err(e) => {
                error!("❌ Failed to insert supervisor log: {}", e);
                Err(ApiError::internal_error("Log not saved"))
            }
        }
    } else {
        Err(ApiError::bad_request("Missing logData field"))
    }
}


/// GET /device/logs
/// 
/// Endpoint to retrieve supervisor logs with optional filtering 
pub async fn get_supervisor_logs(query: web::Query<std::collections::HashMap<String, String>>) -> Result<impl Responder, ApiError> {

    // Optional time filter
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        if let Ok(dt) = DateTime::parse_from_rfc3339(after) {
            let dt_utc = dt.with_timezone(&Utc);
            filter = doc! { "dateReceived": { "$gt": mongodb::bson::DateTime::from_chrono(dt_utc) } };
        }
    }

    let collection = get_collection::<Document>(COLL_LOGS).await;

    match collection.find(filter).await {
        Ok(cursor) => {
            let logs: Vec<Document> = cursor.try_collect().await.unwrap_or_default();
            let mut v = serde_json::to_value(&logs).map_err(ApiError::internal_error)?;
            crate::lib::utils::normalize_object_ids(&mut v);
            Ok(HttpResponse::Ok().json(v))
        }
        Err(e) => {
            error!("❌ Failed to fetch supervisor logs: {}", e);
            Err(ApiError::internal_error("Failed to fetch logs"))
        }
    }
}

