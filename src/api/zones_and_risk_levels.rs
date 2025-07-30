use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Document};
use crate::lib::mongodb::get_collection;
use futures::stream::TryStreamExt;

/// Structure for zones and their allowed risk levels
#[derive(Debug, Serialize, Deserialize)]
pub struct ZoneRiskMapping {
    pub zone: String,
    pub allowed_risk_levels: Vec<String>,
}

/// Structure for risk levels
#[derive(Debug, Serialize, Deserialize)]
pub struct RiskLevelsMetadata {
    pub levels: Vec<String>,
    pub last_updated: DateTime<Utc>,
}

/// Parses zones and risk levels from an ODRL document and saves them to the database
pub async fn parse_zones_and_risk_levels(card: web::Json<Value>) -> impl Responder {
    log::info!("Received zone and risk-level definitions: {:?}", card);

    // Extract zones and risk levels
    let (zone_risk_mappings, risk_levels) = extract_zone_and_risk_level_mappings(&card);

    let collection = get_collection::<Document>("zones").await;

    // Save zones and risk levels to the database
    for zone in &zone_risk_mappings {
        let _ = collection.update_one(
            doc! { "zone": &zone.zone },
            doc! { "$set": { "allowed_risk_levels": &zone.allowed_risk_levels } }
        ).upsert(true).await;
    }
    let risk_levels_metadata = RiskLevelsMetadata {
        levels: risk_levels.clone(),
        last_updated: Utc::now(),
    };
    let _ = collection.update_one(
        doc! { "type": "riskLevels" },
        doc! { "$set": mongodb::bson::to_document(&risk_levels_metadata).unwrap() },
    ).upsert(true).await;
    HttpResponse::Ok().json(json!({
        "message": "Zone and risk-level definitions parsed and saved successfully",
        "zones": zone_risk_mappings,
        "riskLevels": risk_levels
    }))
}

/// Extracts zones and risk levels from an ODRL document
fn extract_zone_and_risk_level_mappings(card: &Value) -> (Vec<ZoneRiskMapping>, Vec<String>) {
    let mut zone_risk_mappings: Vec<ZoneRiskMapping> = Vec::new();
    let mut risk_levels_set = std::collections::BTreeSet::new();

    if let Some(permissions) = card.get("permission").and_then(|p| p.as_array()) {
        for permission in permissions {
            let risk_level = permission.get("target")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string();
            risk_levels_set.insert(risk_level.clone());

            if let Some(constraints) = permission.get("constraint").and_then(|c| c.as_array()) {
                for constraint in constraints {
                    if constraint.get("leftOperand").and_then(|l| l.as_str()) == Some("zone") {
                        let right_operand = constraint.get("rightOperand");
                        let allowed_zones: Vec<String> = match right_operand {
                            Some(Value::Array(arr)) => arr.iter()
                                .filter_map(|z| z.as_str().map(|s| s.to_string()))
                                .collect(),
                            Some(Value::String(s)) => vec![s.clone()],
                            _ => vec![],
                        };
                        for zone in allowed_zones {
                            if let Some(existing_zone) = zone_risk_mappings.iter_mut().find(|z| z.zone == zone) {
                                existing_zone.allowed_risk_levels.push(risk_level.clone());
                            } else {
                                zone_risk_mappings.push(ZoneRiskMapping {
                                    zone: zone,
                                    allowed_risk_levels: vec![risk_level.clone()],
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let risk_levels: Vec<String> = risk_levels_set.into_iter().collect();
    (zone_risk_mappings, risk_levels)
}

/// Get all zones and risk-levels
pub async fn get_zones_and_risk_levels() -> impl Responder {
    let collection = get_collection::<Document>("zones").await;
    let mut cursor = match collection.find(doc! { "zone": { "$exists": true } }).await {
        Ok(cursor) => cursor,
        Err(e) => {
            log::error!("Error querying zones: {}", e);
            return HttpResponse::InternalServerError().json(json!({ "message": "Error querying zones" }));
        }
    };
    let mut zones = Vec::new();
    while let Some(doc) = cursor.try_next().await.unwrap_or(None) {
        match mongodb::bson::from_document::<ZoneRiskMapping>(doc) {
            Ok(zone) => zones.push(zone),
            Err(e) => {
                log::warn!("Failed to deserialize zone mapping: {}", e);
                continue;
            }
        }
    }
    let risk_levels_doc = collection.find_one(doc! { "type": "riskLevels" }).await.ok().flatten();
    let risk_levels = risk_levels_doc
        .and_then(|doc| mongodb::bson::from_document::<RiskLevelsMetadata>(doc).ok());

    HttpResponse::Ok().json(json!({
        "zones": zones,
        "riskLevels": risk_levels
    }))
}

/// Delete all zones and risk-levels
pub async fn delete_all_zones_and_risk_levels() -> impl Responder {
    let collection = get_collection::<Document>("zones").await;
    match collection.delete_many(doc! {}).await {
        Ok(result) => HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count })),
        Err(e) => {
            log::error!("Failed to delete all zones and risk levels: {}", e);
            HttpResponse::InternalServerError().json(json!({ "message": "Failed to delete zones and risk levels" }))
        }
    }
}
