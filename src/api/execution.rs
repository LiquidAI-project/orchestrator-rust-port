use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::doc;
use serde_json;
use futures::TryStreamExt;
use crate::lib::mongodb::get_collection;
use reqwest::{self, Url, Method};
use reqwest::multipart::{Form, Part};
use tokio::fs;
use serde_json::Value;
use serde_json::json;
use actix_web::{web, HttpResponse, Responder};
use actix_web::{HttpRequest};
use actix_web::http::header::CONTENT_TYPE;
use actix_multipart::Multipart;
use futures_util::{StreamExt as FutTryStreamExt};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt as _;
use crate::structs::deployment::{DeploymentDoc, OperationRequest};
use crate::structs::openapi::OpenApiParameterIn;
use crate::lib::errors::ApiError;
use crate::lib::constants::COLL_DEPLOYMENT;

#[derive(Debug, Clone)]
pub struct ScheduleFile {
    pub path: std::path::PathBuf,
    pub name: String,
}


// TODO: These uploaded files should be also deleted at some point.
// TODO: Current UI doesnt really allow testing this part
/// Helper function that takes an uploaded file and saves it to disk
/// Meant to be used for execution mounts that are directly uploaded through 
/// execution UI
async fn save_upload_part(
    field: &mut actix_multipart::Field,
    dir: &std::path::Path,
    original_filename: &str,
) -> Result<PathBuf, ApiError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| ApiError::db(format!("create upload dir failed: {e}")))?;

    let ts = chrono::Utc::now().timestamp_micros();
    let safe = original_filename.replace(['/', '\\', '\0'], "_");
    let filepath = dir.join(format!("{ts}_{safe}"));

    let mut f = tokio::fs::File::create(&filepath)
        .await
        .map_err(|e| ApiError::db(format!("open upload file failed: {e}")))?;

    while let Some(chunk) = field.try_next().await.map_err(|e| {
        ApiError::bad_request(format!("reading file chunk failed: {e}"))
    })? {
        f.write_all(&chunk)
            .await
            .map_err(|e| ApiError::db(format!("write upload failed: {e}")))?;
    }
    Ok(filepath)
}


/// Helper function to parse multipart requests made to the execution endpoint
async fn parse_multipart(
    mut mp: Multipart,
) -> Result<(HashMap<String, String>, Vec<ScheduleFile>), ApiError> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut files: Vec<ScheduleFile> = Vec::new();
    let base_dir = std::env::temp_dir().join("exec_inputs");

    while let Some(mut field) = mp.try_next().await.map_err(|e| {
        ApiError::bad_request(format!("multipart error: {e}"))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        if let Some(cd) = field.content_disposition().cloned() {
            if let Some(fname) = cd.get_filename() {
                let saved = save_upload_part(&mut field, &base_dir, fname).await?;
                files.push(ScheduleFile {
                    path: saved,
                    name: field_name.clone(),
                });
                continue;
            }
        }

        let mut buf = Vec::new();
        while let Some(chunk) = field.try_next().await.map_err(|e| {
            ApiError::bad_request(format!("multipart field read failed: {e}"))
        })? {
            buf.extend_from_slice(&chunk);
        }
        let val = String::from_utf8_lossy(&buf).to_string();
        fields.insert(field_name, val);
    }

    Ok((fields, files))
}


/// Helper function to parse requests to execution endpoint that are not multipart requests
async fn parse_non_multipart_body(
    mut payload: web::Payload,
) -> Result<HashMap<String, String>, ApiError> {
    let mut bytes = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let c = chunk.map_err(|e| ApiError::bad_request(format!("read body failed: {e}")))?;
        bytes.extend_from_slice(&c);
    }

    if bytes.is_empty() {
        return Ok(HashMap::new());
    }

    let v: Result<Value, _> = serde_json::from_slice(&bytes);
    match v {
        Ok(Value::Object(map)) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                let s = match v {
                    Value::Null => String::new(),
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                out.insert(k, s);
            }
            Ok(out)
        }
        Ok(other) => Ok(HashMap::from([("body".into(), other.to_string())])),
        Err(_) => Ok(HashMap::from([(
            "body".into(),
            String::from_utf8_lossy(&bytes).to_string(),
        )])),
    }
}


/// POST /execute/{deployment_id}
/// 
/// Endpoint to handle executing a deployment. Assumes that a deployment has already been deployed to 
/// the target devices.
pub async fn execute(
    path: web::Path<String>,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<impl Responder, ApiError> {
    let deployment_param = path.into_inner();
    let coll = get_collection::<DeploymentDoc>(COLL_DEPLOYMENT).await;

    let filter = match ObjectId::parse_str(&deployment_param) {
        Ok(oid) => doc! { "_id": oid },
        Err(_) => doc! { "name": &deployment_param },
    };

    let Some(deployment) = coll
        .find_one(filter)
        .await
        .map_err(ApiError::db)?
    else {
        return Ok(HttpResponse::NotFound().finish());
    };

    let (.., _, _, start_req) =
        crate::api::execution::get_start_endpoint(&deployment)
            .map_err(|e| ApiError::db(e))?;
    let expects_request_body = start_req.request_body.is_some();

    let ct = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    let (fields, files): (HashMap<String, String>, Vec<ScheduleFile>) =
        if ct.starts_with("multipart/form-data") {
            match <actix_multipart::Multipart as actix_web::FromRequest>
                ::from_request(&req, &mut payload.into_inner())
                .await
            {
                Ok(mp) => match parse_multipart(mp).await {
                    Ok(t) => t,
                    Err(e) => {
                        if expects_request_body {
                            return Err(e);
                        } else {
                            (HashMap::new(), Vec::new())
                        }
                    }
                },
                Err(e) => {
                    if expects_request_body {
                        return Err(ApiError::bad_request(format!("invalid multipart payload: {e}")));
                    } else {
                        (HashMap::new(), Vec::new())
                    }
                }
            }
        } else {
            (parse_non_multipart_body(payload).await?, Vec::new())
        };

    let exec_response = schedule(&deployment, &fields, &files)
        .await
        .map_err(|e| ApiError::db(format!("scheduling work failed: {e}")))?;

    if !exec_response.status().is_success() {
        let txt = exec_response
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".into());
        return Err(ApiError::db(format!("scheduling work failed: {}", txt)));
    }

    let client = reqwest::Client::new();
    let mut resp = exec_response;
    let mut tries = 0usize;
    let mut depth = 0usize;
    let mut status_code = 500;
    let mut _result: Value = json!({ "error": "undefined error" });

    loop {
        let json_res: Result<Value, _> = resp.json().await;
        let json = match json_res {
            Ok(v) => v,
            Err(e) => {
                _result = json!({ "error": format!("parsing result to JSON failed: {e}") });
                break;
            }
        };

        if let Some(res_val) = json.get("result") {
            if json.get("status").and_then(Value::as_str) != Some("error") {
                if let Some(res_str) = res_val.as_str() {
                    if let Ok(url) = Url::parse(res_str) {
                        depth += 1;
                        let next = client.get(url).send().await.map_err(|e| {
                            ApiError::db(format!("fetching result failed: {e}"))
                        })?;
                        if !next.status().is_success() {
                            if next.status().as_u16() == 404 && depth < 5 && tries < 5 {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                tries += 1;
                                resp = client
                                    .get(next.url().clone())
                                    .send()
                                    .await
                                    .map_err(|e| ApiError::db(format!("retry failed: {e}")))?;
                                continue;
                            } else {
                                _result = json!({ "error": format!("fetching result failed: {}", next.status()) });
                                break;
                            }
                        }
                        resp = next;
                        continue;
                    }
                }
                _result = res_val.clone();
                status_code = 200;
                break;
            }
        }

        if let Some(err) = json.get("error") {
            _result = json!({ "error": err });
            break;
        }

        if let Some(url_val) = json.get("resultUrl").and_then(Value::as_str) {
            if let Ok(url) = Url::parse(url_val) {
                depth += 1;
                let next = client.get(url).send().await.map_err(|e| {
                    ApiError::db(format!("fetching result failed: {e}"))
                })?;
                if !next.status().is_success() {
                    if next.status().as_u16() == 404 && depth < 5 && tries < 5 {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        tries += 1;
                        resp = client
                            .get(next.url().clone())
                            .send()
                            .await
                            .map_err(|e| ApiError::db(format!("retry failed: {e}")))?;
                        continue;
                    } else {
                        _result =
                            json!({ "error": format!("fetching result failed: {}", next.status()) });
                        break;
                    }
                }
                resp = next;
                continue;
            }
        }

        _result = json!({ "error": "unexpected execution response shape" });
        break;
    }

    Ok(HttpResponse::build(
        actix_web::http::StatusCode::from_u16(status_code).unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
    )
    .json(_result))
}


/// Start execution on the first device of the deployment chain.
pub async fn schedule(
    deployment: &DeploymentDoc,
    body: &HashMap<String, String>,
    files: &[ScheduleFile],
) -> Result<reqwest::Response, String> {
    let (mut url, mut path, method_str, request) = get_start_endpoint(deployment)?;

    for param in &request.parameters {
        let name = &param.name;
        let val = body.get(name).ok_or_else(|| {
            format!("parameter missing: name='{}' in='{:?}' on path '{}'", name, param.r#in, path)
        })?;
        match param.r#in {
            OpenApiParameterIn::Path => {
                let with_braces = format!("{{{}}}", name);
                if path.contains(&with_braces) {
                    path = path.replace(&with_braces, val);
                } else {
                    path = path.replace(name, val);
                }
            }
            OpenApiParameterIn::Query => {
                url.query_pairs_mut().append_pair(name, val);
            }
            _ => return Err(format!("parameter location not supported: '{:?}'", param.r#in)),
        }
    }

    url.set_path(&path);

    let client = reqwest::Client::new();
    let method = match method_str.to_ascii_lowercase().as_str() {
        "get" => Method::GET,
        "head" => Method::HEAD,
        "post" => Method::POST,
        "put" => Method::PUT,
        "delete" => Method::DELETE,
        "patch" => Method::PATCH,
        "options" => Method::OPTIONS,
        "trace" => Method::TRACE,
        m => return Err(format!("unsupported HTTP method '{}'", m)),
    };

    let mut req = client.request(method.clone(), url);

    // Set the X-Chain-Step header to 0, to indicate to supervisor that it needs to 
    // execute the first step.
    req = req.header("X-Chain-Step", "0");

    if method != Method::GET && method != Method::HEAD {
        if request.request_body.is_some() {
            let mut form = Form::new();
            for f in files {
                let bytes = fs::read(&f.path)
                    .await
                    .map_err(|e| format!("failed to read file '{}': {e}", f.path.display()))?;
                let part = Part::bytes(bytes).file_name(f.name.clone());
                form = form.part(f.name.clone(), part);
            }
            req = req.multipart(form);
        } else {
            req = req.json(&serde_json::json!({ "foo": "bar" }));
        }
    }

    req.send()
        .await
        .map_err(|e| format!("request failed: {e}"))
}


/// Get the starting endpoint from a Deployment
/// 
/// Returns (base_url, path, method, openapi_request)
/// - base_url: Url (scheme + host + port), for example http://example.com
/// - path: String (the path template for the endpoint), for example /{deployment_id}/modules/{module_name}/{function_name}
/// - method: String (the HTTP method for the endpoint), for example 'get' or 'post'
/// - a list of openapi parameter objects, for example {'parameters': [OpenApiParameterEnum]}
fn get_start_endpoint(
    deployment: &DeploymentDoc,
) -> Result<(Url, String, String, OperationRequest), String> {

    // Get the first device under the "sequence" key of a deployment
    let start = deployment
        .sequence
        .get(0)
        .ok_or_else(|| "Deployment had an empty sequence".to_string())?;

    // Find the corresponding entry under "fullManifest" key
    let device_hex = start.device.to_hex();
    let node = deployment
        .full_manifest
        .get(&device_hex)
        .ok_or_else(|| format!("device '{}' not found in fullManifest", device_hex))?;

    // Find the name of the starting module. The modules are in a list, so find the 
    // module in the list with an id that matches the module in the first item of the 
    // sequence (first step of this function)
    let module_name = node
        .modules
        .iter()
        .find(|m| m.id == start.module)
        .map(|m| m.name.clone())
        .ok_or_else(|| {
            format!(
                "module '{}' not found on device '{}'",
                start.module.to_hex(),
                device_hex
            )
        })?;

    // Get the endpoint information for the starting module/function. The endpoints
    // are stored as a map of module name -> function name -> endpoint information.
    let ep = node
        .endpoints
        .get(&module_name)
        .and_then(|m| m.get(&start.func))
        .ok_or_else(|| {
            format!(
                "endpoint not found for module '{}' func '{}' on device '{}'",
                module_name, start.func, device_hex
            )
        })?;

    // Parse the url from the endpoint information that was just fetched
    let url = Url::parse(&ep.url)
        .map_err(|e| format!("invalid endpoint url '{}': {e}", ep.url))?;

    Ok((url, ep.path.clone(), ep.method.clone(), ep.request.clone()))
}
