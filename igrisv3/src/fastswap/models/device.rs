use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    Http,
    Https,
}

impl ProtocolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolType::Http => "http",
            ProtocolType::Https => "https",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mobile,
    Desktop,
    Web,
    Headless,
}

/// Internal device representation (not a wire DTO).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub alias: String,
    pub device_model: String,
    pub device_type: DeviceType,
    pub ip: String,
    pub port: u16,
    pub version: String,
    pub protocol: ProtocolType,
    pub download: bool,
    pub fingerprint: String,
}

impl Device {
    pub fn new_local(alias: String, port: u16, ip: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            alias,
            device_model: whoami::devicename(),
            device_type: DeviceType::Desktop,
            ip,
            port,
            version: "2.0".to_string(),
            protocol: ProtocolType::Http,
            download: false,
            fingerprint: Uuid::new_v4().to_string(),
        }
    }
}

/// Register request DTO — POST /api/localsend/v2/register
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub alias: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    pub port: u16,
    pub protocol: ProtocolType,
    pub download: bool,
}

/// Register response DTO — response from POST /api/localsend/v2/register
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterResponse {
    pub alias: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    #[serde(default)]
    pub download: bool,
}

/// Info response DTO — GET /api/localsend/v2/info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoResponse {
    pub alias: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    #[serde(default)]
    pub download: bool,
}

/// UDP multicast announcement/response message (v2.1 protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticastMessageV2 {
    pub alias: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    pub port: u16,
    pub protocol: ProtocolType,
    #[serde(default)]
    pub download: bool,
    pub announce: bool,
}
