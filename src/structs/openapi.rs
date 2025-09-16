use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use serde_json::Value;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiDocument {
    pub openapi: OpenApiVersion,
    pub info: OpenApiInfo,
    #[serde(skip_serializing_if="Option::is_none")]
    pub servers: Option<Vec<OpenApiServerObject>>,
    pub paths: HashMap<String, OpenApiPathItemObject>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub components: Option<OpenApiComponents>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub security: Option<Vec<OpenApiSecurityRequirementObject>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub tags: Option<Vec<OpenApiTagObject>>,
    #[serde(rename="externalDocs", skip_serializing_if="Option::is_none")]
    pub external_docs: Option<OpenApiExternalDocs>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenApiVersion {
    // #[serde(rename = "3.1.1")] V3_1_1,
    // #[serde(rename = "3.1.0")] V3_1_0,
    // #[serde(rename = "3.0.4")] V3_0_4,
    #[serde(rename = "3.0.3")] V3_0_3,
    // #[serde(rename = "3.0.2")] V3_0_2,
    // #[serde(rename = "3.0.1")] V3_0_1,
    // #[serde(rename = "3.0.0")] V3_0_0,
    // #[serde(rename = "2.0")] V2_0,
    // #[serde(rename = "1.2")] V1_2,
    // #[serde(rename = "1.1")] V1_1,
    // #[serde(rename = "1.0")] V1_0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub title: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(rename="termsOfService", skip_serializing_if="Option::is_none")]
    pub terms_of_service: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub contact: Option<OpenApiContactInfo>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub license: Option<OpenApiLicenseInfo>,
    pub version: String
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiContactInfo {
    #[serde(skip_serializing_if="Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub email: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiLicenseInfo {
    pub name: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub url: Option<String>,
}

/// https://spec.openapis.org/oas/v3.0.3.html#path-item-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiPathItemObject {
    #[serde(rename="$ref", skip_serializing_if="Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub get: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub put: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub post: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub delete: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub options: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub head: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub patch: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub trace: Option<OpenApiOperation>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub servers: Option<Vec<OpenApiServerObject>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub parameters: Option<Vec<OpenApiParameterEnum>>
}

/// https://spec.openapis.org/oas/v3.0.3.html#operation-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiOperation {
    #[serde(default)]
    pub tags: Vec<String>, // NOTE: This could be optional, but in original orchestrator tags: [] is always present
    #[serde(skip_serializing_if="Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(rename= "externalDocs", skip_serializing_if="Option::is_none")]
    pub external_docs: Option<OpenApiExternalDocs>,
    #[serde(rename= "operationId", skip_serializing_if="Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub parameters: Option<Vec<OpenApiParameterEnum>>,
    #[serde(rename="requestBody", skip_serializing_if="Option::is_none")]
    pub request_body: Option<RequestBodyEnum>,
    pub responses: HashMap<String, ResponseEnum>, //NOTE: This maps http response codes like '200' into response objects
    #[serde(skip_serializing_if="Option::is_none")]
    pub callbacks: Option<HashMap<String, OpenApiCallbackEnum>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub deprecated: Option<bool>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub security: Option<Vec<OpenApiSecurityRequirementObject>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub servers: Option<Vec<OpenApiServerObject>>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenApiCallbackEnum {
    // NOTE: Each Callback object is either a reference, or 
    // is a path object (mapped to some path like /xyz/foo).
    // https://spec.openapis.org/oas/v3.0.3.html#callback-object
    OpenApiPathItemObject(OpenApiPathItemObject), 
    OpenApiReferenceObject(OpenApiReferenceObject)
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiCallbackObject {
    // TODO: Fill out if necessary some day
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseEnum {
    OpenApiResponseObject(OpenApiResponseObject),
    OpenApiReferenceObject(OpenApiReferenceObject)
}

/// https://spec.openapis.org/oas/v3.0.3.html#response-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiResponseObject {
    pub description: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub headers: Option<HashMap<String, OpenApiHeaderEnum>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub content: Option<HashMap<String, OpenApiMediaTypeObject>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub links: Option<HashMap<String, OpenApiLinkEnum>>
}

/// https://spec.openapis.org/oas/v3.0.3.html#link-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiLinkObject {
    #[serde(rename="operationRef", skip_serializing_if="Option::is_none")]
    pub operation_ref: Option<String>,
    #[serde(rename="operationId", skip_serializing_if="Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub parameters: Option<HashMap<String, Value>>,
    #[serde(rename="requestBody", skip_serializing_if="Option::is_none")]
    pub request_body: Option<Value>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub server: Option<OpenApiServerObject>
}

/// https://spec.openapis.org/oas/v3.0.3.html#server-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiServerObject {
    pub url: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub variables: Option<HashMap<String, OpenApiServerVariableObject>>
}

/// https://spec.openapis.org/oas/v3.0.3.html#server-variable-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiServerVariableObject {
    #[serde(rename="enum", skip_serializing_if="Option::is_none")]
    pub r#enum: Option<Vec<String>>,
    pub default: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenApiLinkEnum {
    OpenApiLinkObject(OpenApiLinkObject),
    OpenApiReferenceObject(OpenApiReferenceObject)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestBodyEnum {
    OpenApiRequestBodyObject(OpenApiRequestBodyObject),
    OpenApiReferenceObject(OpenApiReferenceObject)
}

/// https://spec.openapis.org/oas/v3.0.3.html#request-body-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiRequestBodyObject {
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    pub content: HashMap<String, OpenApiMediaTypeObject>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub required: Option<bool>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenApiParameterEnum {
    OpenApiParameterObject(OpenApiParameterObject),
    OpenApiReferenceObject(OpenApiReferenceObject)
}

/// A combination of openapi definitions and orchestrators requirements (the requestbody is added)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OpenApiParameterIn {
    #[serde(rename="query")]
    Query,
    #[serde(rename="header")]
    Header,
    #[serde(rename="path")]
    Path,
    #[serde(rename="cookie")]
    Cookie,
    #[serde(rename="requestBody")]
    RequestBody
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenApiSchemaEnum {
    OpenApiSchemaObject(OpenApiSchemaObject),
    OpenApiReferenceObject(OpenApiReferenceObject)
}

/// https://spec.openapis.org/oas/v3.0.3.html#parameter-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiParameterObject {
    // NOTE: example and examples fields are not implemented here
    // NOTE: Different style related fields are not implemented here
    pub name: String,
    #[serde(rename="in")]
    pub r#in: OpenApiParameterIn,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    pub required: bool, // This field is sometimes optional, sometimes not (in specification). Set as mandatory here though.
    #[serde(skip_serializing_if="Option::is_none")]
    pub deprecated: Option<bool>,
    #[serde(rename="allowEmptyValue", skip_serializing_if="Option::is_none")]
    pub allow_empty_value: Option<bool>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub explode: Option<bool>,
    #[serde(rename="allowReserved", skip_serializing_if="Option::is_none")]
    pub allow_reserved: Option<bool>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub schema: Option<OpenApiSchemaEnum>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub content: Option<HashMap<String, OpenApiMediaTypeObject>>
}

/// https://spec.openapis.org/oas/v3.0.3.html#media-type-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiMediaTypeObject {
    // NOTE: example and examples fields are not implemented here
    #[serde(skip_serializing_if="Option::is_none")]
    pub schema: Option<OpenApiSchemaEnum>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub encoding: Option<HashMap<String, OpenApiEncodingObject>>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenApiHeaderEnum {
    // NOTE: Header object is same as parameter object with some caveats on how its used.
    // More info on that: https://spec.openapis.org/oas/v3.0.3.html#header-object
    OpenApiHeaderObject(OpenApiParameterObject),
    OpenApiReferenceObject(OpenApiReferenceObject)
}


///https://spec.openapis.org/oas/v3.0.3.html#encoding-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiEncodingObject {
    #[serde(rename="contentType", skip_serializing_if="Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub headers: Option<HashMap<String, OpenApiHeaderEnum>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub explode: Option<bool>,
    #[serde(rename="allowReserved", skip_serializing_if="Option::is_none")]
    pub allow_reserved: Option<bool>
}

/// https://spec.openapis.org/oas/v3.0.3.html#reference-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiReferenceObject {
    #[serde(rename="$ref")]
    pub r#ref: String
}

/// Combination of what the orchestrator needs, and what the openapi specification defines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenApiFormat {
    #[serde(rename="int32")]
    Int32,
    #[serde(rename="int64")]
    Int64,
    #[serde(rename="float")]
    Float,
    #[serde(rename="double")]
    Double,
    #[serde(rename="byte")]
    Byte,
    #[serde(rename="binary")]
    Binary,
    #[serde(rename="boolean")]
    Boolean,
    #[serde(rename="date")]
    Date,
    #[serde(rename="date-time")]
    DateTime,
    #[serde(rename="password")]
    Password,
    #[serde(rename="object")]
    Object,
    #[serde(rename="string")]
    String,
    #[serde(rename="integer")]
    Integer,
}

/// https://spec.openapis.org/oas/v3.0.3.html#schema-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSchemaObject {
    // NOTE: This is not fully implemented here because it doesnt appear necessary for orchestrator
    // functionality. Only parts that are used are implemented here.
    #[serde(rename="type", skip_serializing_if="Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub properties: Option<HashMap<String, OpenApiSchemaEnum>>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub format: Option<OpenApiFormat>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiComponents {
    // TODO: When necessary
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSecurityRequirementObject {
    // TODO: When necessary
}

/// https://spec.openapis.org/oas/v3.0.3.html#external-documentation-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiExternalDocs {
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub url: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiTagObject {
    pub name: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
    #[serde(rename="externalDocs", skip_serializing_if="Option::is_none")]
    pub external_docs: Option<OpenApiExternalDocs>
}