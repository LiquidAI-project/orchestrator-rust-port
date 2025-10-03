use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use chrono::Utc;
use mongodb::bson::doc;
use futures::stream::TryStreamExt;
use crate::lib::mongodb::get_collection;
use crate::structs::zones::Zones;
use crate::lib::errors::ApiError;
use crate::lib::constants::COLL_ZONES;
use log::{debug, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct ZoneRiskMapping {
    pub zone: String,
    pub allowed_risk_levels: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RiskLevelsMetadata {
    pub levels: Vec<String>,
    pub last_updated: chrono::DateTime<Utc>,
}


/// POST /zoneRiskLevels
/// 
/// Endpoint for receiving and parsing a json that contains the zone and risk level definitions
pub async fn parse_zones_and_risk_levels(card: web::Json<Value>) -> Result<impl Responder, ApiError> {
    debug!("Received zone and risk-level definitions: {:?}", card);

    let (zone_risk_mappings, risk_levels) = extract_zone_and_risk_level_mappings(&card);
    let collection = get_collection::<Zones>(COLL_ZONES).await;
    let now = Utc::now();

    for zone in &zone_risk_mappings {
        let z = Zones {
            id: None,
            zone: Some(zone.zone.clone()),
            allowed_risk_levels: Some(zone.allowed_risk_levels.clone()),
            r#type: None,
            last_updated: now,
            levels: None,
        };
        let set_doc = mongodb::bson::to_document(&z).expect("serialize zone doc");
        let _ = collection
            .update_one(
                doc! { "zone": &zone.zone },
                doc! { "$set": set_doc }
            )
            .upsert(true)
            .await;
    }

    let risk_levels_doc = Zones {
        id: None,
        zone: None,
        allowed_risk_levels: None,
        r#type: Some("riskLevels".to_string()),
        last_updated: now,
        levels: Some(risk_levels.clone()),
    };
    let set_doc = mongodb::bson::to_document(&risk_levels_doc).expect("serialize riskLevels doc");
    let _ = collection
        .update_one(
            doc! { "type": "riskLevels" },
            doc! { "$set": set_doc }
        )
        .upsert(true)
        .await;

    Ok(HttpResponse::Ok().json(json!({
        "message": "Zone and risk-level definitions parsed and saved successfully",
        "zones": zone_risk_mappings,
        "riskLevels": RiskLevelsMetadata { levels: risk_levels, last_updated: now },
    })))
}


/// Helper function for extracting the zones and risk levels from a given json
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
                                    zone,
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


/// GET /zoneRiskLevels
/// 
/// Endpoint for getting the zone and risk level definitions
pub async fn get_zones_and_risk_levels() -> Result<impl Responder, ApiError> {
    let collection = get_collection::<Zones>(COLL_ZONES).await;
    let mut cursor = match collection.find(doc! { "zone": { "$exists": true } }).await {
        Ok(cursor) => cursor,
        Err(e) => {
            error!("Error querying zones: {}", e);
            return Err(ApiError::internal_error("Error querying zones"));
        }
    };
    let mut zones_out = Vec::new();
    while let Some(doc) = cursor.try_next().await.unwrap_or(None) {
        if let (Some(zone), Some(allowed)) = (doc.zone.clone(), doc.allowed_risk_levels.clone()) {
            zones_out.push(ZoneRiskMapping {
                zone,
                allowed_risk_levels: allowed,
            });
        }
    }

    let risk_levels_doc = get_collection::<Zones>(COLL_ZONES)
        .await
        .find_one(doc! { "type": "riskLevels" })
        .await
        .ok()
        .flatten();
    let risk_levels = risk_levels_doc.as_ref().map(|z| RiskLevelsMetadata {
        levels: z.levels.clone().unwrap_or_default(),
        last_updated: z.last_updated,
    });

    Ok(HttpResponse::Ok().json(json!({
        "zones": zones_out,
        "riskLevels": risk_levels
    })))
}


/// DELETE /zoneRiskLevels
/// 
/// Endpoint for deleting all zones and risk levels
pub async fn delete_all_zones_and_risk_levels() -> Result<impl Responder, ApiError> {
    let collection = get_collection::<Zones>(COLL_ZONES).await;
    match collection.delete_many(doc! {}).await {
        Ok(result) => Ok(HttpResponse::Ok().json(json!({ "deleted_count": result.deleted_count }))),
        Err(e) => {
            error!("Failed to delete all zones and risk levels: {}", e);
            Err(ApiError::internal_error("Failed to delete zones and risk levels"))
        }
    }
}
