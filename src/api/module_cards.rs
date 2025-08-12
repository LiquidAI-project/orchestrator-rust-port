use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Document};
use mongodb::IndexModel;
use crate::lib::mongodb::get_collection;
use futures::stream::TryStreamExt;
use log::{debug, info, error};


/// Struct to hold relevant module card information
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "moduleData")]
pub struct ModuleCard {
    pub moduleid: String,
    pub name: String,
    #[serde(rename = "risk-level", skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(rename = "input-type", skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    #[serde(rename = "output-risk", skip_serializing_if = "Option::is_none")]
    pub output_risk: Option<String>,
    #[serde(rename = "dateReceived", with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub date_received: DateTime<Utc>,
}


/// Endpoint for creating a new module card
pub async fn create_module_card(body: web::Json<Value>) -> impl Responder {
    debug!("Received module card data: {:?}", body);

    // Check that permission exists in received document
    let perm = match body.get("permission").and_then(|p| p.as_array()).and_then(|a| a.get(0)) {
        Some(p) => p,
        None => {
            return HttpResponse::BadRequest()
                .json(json!({"message":"Invalid ODRL document: Missing or invalid 'permission' section."}));
        }
    };

    // Check that the permission contains fields 'target', 'action', and 'constraint'
    let target = match perm.get("target").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return HttpResponse::BadRequest().json(json!({"message":"Invalid permission: missing 'target'"})),
    };
    let action = match perm.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return HttpResponse::BadRequest().json(json!({"message":"Invalid permission: missing 'action'"})),
    };
    let constraints = match perm.get("constraint").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return HttpResponse::BadRequest().json(json!({"message":"Invalid permission: missing 'constraint' array"})),
    };

    // Map the constraints.
    // TODO: Should the operator be ignored, or is it always 'eq'?
    let mut risk_level: Option<String> = None;
    let mut input_type: Option<String> = None;
    let mut output_risk: Option<String> = None;
    for c in constraints {
        let left = c.get("leftOperand").and_then(|v| v.as_str());
        let right = c.get("rightOperand").and_then(|v| v.as_str());
        if let (Some(l), Some(r)) = (left, right) {
            match l {
                "risk-level" => risk_level = Some(r.to_string()),
                "input-type" => input_type = Some(r.to_string()),
                "output-risk" => output_risk = Some(r.to_string()),
                _ => {}
            }
        }
    }

    // Create the ModuleCard, serialize it, and save it to database
    let module_card = ModuleCard {
        moduleid: target.to_string(),
        name: action.to_string(),
        risk_level,
        input_type,
        output_risk,
        date_received: Utc::now(),
    };
    let doc = match mongodb::bson::to_document(&module_card) {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to serialize ModuleCard: {}", e);
            return HttpResponse::InternalServerError().json(json!({"message":"Serialization error"}));
        }
    };
    let coll = get_collection::<Document>("modulecards").await;
    match coll.insert_one(doc).await {
        Ok(_) => {
            info!("Module card received and saved successfully. Saved card:\n{:?}", module_card);
            HttpResponse::Ok().json(json!({ "message": "Module card received and saved", "moduleCard": module_card }))
        },
        Err(e) => {
            error!("Error inserting module card: {}", e);
            HttpResponse::InternalServerError().json(json!({ "message": "Error processing module card" }))
        }
    }
}


/// Endpoint for getting module cards. Accepts optional query parameters (e.g., after)
/// Example: GET /modulecards?after=2025-08-12T12:00:00Z
pub async fn get_module_cards(query: web::Query<std::collections::HashMap<String, String>>) -> impl Responder {
    let coll = get_collection::<Document>("modulecards").await;

    // Ensure index on dateReceived exists
    let _ = coll.create_index(IndexModel::builder().keys(doc! { "dateReceived": 1 }).build()).await;

    // Optional time filter
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        if let Ok(dt) = DateTime::parse_from_rfc3339(after) {
            let dt_utc = dt.with_timezone(&Utc);
            filter = doc! { "dateReceived": { "$gt": mongodb::bson::DateTime::from_chrono(dt_utc) } };
        }
    }

    // Get the matching module cards, if any, and return them
    let mut cursor = match coll.find(filter).await {
        Ok(c) => c,
        Err(e) => {
            error!("Error querying module cards: {}", e);
            return HttpResponse::InternalServerError().json(json!({ "message": "Error querying module cards" }));
        }
    };
    let mut out: Vec<ModuleCard> = Vec::new();
    while let Some(doc) = cursor.try_next().await.unwrap_or(None) {
        match mongodb::bson::from_document::<ModuleCard>(doc) {
            Ok(card) => out.push(card),
            Err(e) => log::warn!("Failed to deserialize ModuleCard: {}", e),
        }
    }
    HttpResponse::Ok().json(out)
}


/// Endpoint for deleting all module cards
pub async fn delete_all_module_cards() -> impl Responder {
    let coll = get_collection::<Document>("modulecards").await;
    match coll.delete_many(doc! {}).await {
        Ok(res) => HttpResponse::Ok().json(json!({ "deleted_count": res.deleted_count })),
        Err(e) => {
            error!("Failed to delete all module cards: {}", e);
            HttpResponse::InternalServerError().json(json!({ "message":"Failed to delete module cards" }))
        }
    }
}


/// Endpoint for deleting a single module card by its moduleid
pub async fn delete_module_card_by_id(path: web::Path<String>) -> impl Responder {
    let moduleid = path.into_inner();
    let coll = get_collection::<Document>("modulecards").await;
    match coll.delete_one(doc! { "moduleid": &moduleid }).await {
        Ok(res) if res.deleted_count == 1 => {
            HttpResponse::Ok().json(json!({ "message":"Module card deleted", "moduleid": moduleid }))
        }
        Ok(_) => HttpResponse::NotFound().json(json!({ "message":"Module card not found", "moduleid": moduleid })),
        Err(e) => {
            log::error!("Failed to delete module card {}: {}", moduleid, e);
            HttpResponse::InternalServerError().json(json!({ "message":"Failed to delete module card", "moduleid": moduleid }))
        }
    }
}
