use actix_web::{web, HttpResponse, Responder};
use bson::oid::ObjectId;
use serde_json::{Value, json};
use chrono::{DateTime, Utc};
use mongodb::bson::doc;
use crate::lib::mongodb::get_collection;
use futures::stream::TryStreamExt;
use log::{debug, info, error};
use crate::structs::module_cards::ModuleCard;
use crate::lib::errors::ApiError;
use crate::lib::constants::COLL_MODULE_CARDS;


/// POST /moduleCards
/// 
/// Endpoint for creating a new module card
pub async fn create_module_card(body: web::Json<Value>) -> Result<impl Responder, ApiError> {
    debug!("Received module card data: {:?}", body);

    // Check that permission exists in received document
    let perm = match body.get("permission").and_then(|p| p.as_array()).and_then(|a| a.get(0)) {
        Some(p) => p,
        None => {
            return Err(ApiError::bad_request("Invalid ODRL document: Missing or invalid 'permission' section."));
        }
    };

    // Check that the permission contains fields 'target', 'action', and 'constraint'
    let target = match perm.get("target").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return Err(ApiError::bad_request("Invalid permission: missing 'target'")),
    };
    let action = match perm.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return Err(ApiError::bad_request("Invalid permission: missing 'action'")),
    };
    let constraints = match perm.get("constraint").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return Err(ApiError::bad_request("Invalid permission: missing 'constraint' array")),
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

    // Parse moduleid as ObjectId
    let moduleid = match ObjectId::parse_str(target) {
        Ok(oid) => oid,
        Err(_) => {
            return Err(ApiError::bad_request("Invalid 'target': must be a valid MongoDB ObjectId string"));
        }
    };

    // Create the ModuleCard, serialize it, and save it to database
    let module_card = ModuleCard {
        id: None,
        moduleid,
        name: action.to_string(),
        risk_level: risk_level.unwrap_or_default(),
        input_type: input_type.unwrap_or_default(),
        output_risk: output_risk.unwrap_or_default(),
        date_received: Utc::now(),
    };

    let coll = get_collection::<ModuleCard>(COLL_MODULE_CARDS).await;
    match coll.insert_one(&module_card).await {
        Ok(_) => {
            info!("Module card received and saved successfully. Saved card:\n{:?}", module_card);
            Ok(HttpResponse::Ok().json(json!({ "message": "Module card received and saved", "moduleCard": module_card })))
        },
        Err(e) => {
            error!("Error inserting module card: {}", e);
            Err(ApiError::db("Error while saving module card"))
        }
    }
}


/// GET /moduleCards
/// 
/// Endpoint for getting module cards. Accepts optional query parameters (e.g., after)
/// Example: GET /modulecards?after=2025-08-12T12:00:00Z
pub async fn get_module_cards(query: web::Query<std::collections::HashMap<String, String>>) -> Result<impl Responder, ApiError> {
    let coll = get_collection::<ModuleCard>(COLL_MODULE_CARDS).await;

    // Optional time filter
    let mut filter = doc! {};
    if let Some(after) = query.get("after") {
        match DateTime::parse_from_rfc3339(after) {
            Ok(dt) => {
                let dt_utc = dt.with_timezone(&Utc);
                filter = doc! { "dateReceived": { "$gt": mongodb::bson::DateTime::from_chrono(dt_utc) } };
            }
            Err(e) => {
                return Err(ApiError::bad_request(format!("Invalid 'after' timestamp: {}", e)));
            }
        }
    }

    // Get the matching module cards, if any, and return them
    let mut cursor = match coll.find(filter).await {
        Ok(c) => c,
        Err(e) => {
            error!("Error querying module cards: {}", e);
            return Err(ApiError::internal_error("Error querying module cards"));
        }
    };
    let mut out: Vec<ModuleCard> = Vec::new();
    while let Some(doc) = cursor.try_next().await.unwrap_or(None) {
        out.push(doc);
    }
    let mut v = serde_json::to_value(&out).map_err(ApiError::internal_error)?;
    crate::lib::utils::normalize_object_ids(&mut v);
    Ok(HttpResponse::Ok().json(v))
}


/// DELETE /moduleCards
/// 
/// Endpoint for deleting all module cards
pub async fn delete_all_module_cards() -> Result<impl Responder, ApiError> {
    let coll = get_collection::<ModuleCard>(COLL_MODULE_CARDS).await;
    match coll.delete_many(doc! {}).await {
        Ok(res) => Ok(HttpResponse::Ok().json(json!({ "deleted_count": res.deleted_count }))),
        Err(e) => {
            error!("Failed to delete all module cards: {}", e);
            Err(ApiError::internal_error("Failed to delete module cards"))
        }
    }
}


/// DELETE /moduleCards/{card_id}
/// 
/// Endpoint for deleting a single module card by its moduleid
pub async fn delete_module_card_by_id(path: web::Path<String>) -> Result<impl Responder, ApiError> {
    let moduleid_str = path.into_inner();
    let moduleid = match ObjectId::parse_str(&moduleid_str) {
        Ok(oid) => oid,
        Err(_) => {
            return Err(ApiError::bad_request(format!("Invalid moduleid: must be ObjectId hex string, moduleid: {}", moduleid_str)));
        }
    };
    let coll = get_collection::<ModuleCard>(COLL_MODULE_CARDS).await;
    match coll.delete_one(doc! { "moduleid": &moduleid }).await {
        Ok(res) if res.deleted_count == 1 => {
            Ok(HttpResponse::Ok().json(json!({ "message":"Module card deleted", "moduleid": moduleid })))
        }
        Ok(_) => Err(ApiError::not_found(format!("Module card not found, moduleid: {:?}", moduleid))),
        Err(e) => {
            error!("Failed to delete module card {}: {}", moduleid, e);
            Err(ApiError::internal_error(format!("Failed to delete module card, moduleid: {:?}", moduleid)))
        }
    }
}
