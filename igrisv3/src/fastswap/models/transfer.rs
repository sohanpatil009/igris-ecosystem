use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub file_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FileMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadRequest {
    pub info: super::RegisterRequest,
    pub files: HashMap<String, FileDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadResponse {
    pub session_id: String,
    pub files: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmUploadRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmUploadResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDownloadResponse {
    pub info: super::InfoResponse,
    pub session_id: String,
    pub files: HashMap<String, FileDto>,
}

#[derive(Debug, Clone)]
pub struct TransferState {
    pub session_id: String,
    pub files: Vec<FileTransfer>,
    pub total_size: u64,
    pub transferred: u64,
    pub status: TransferStatus,
    pub confirmed: bool,
    pub encryption_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileTransfer {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub transferred: u64,
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransferStatus {
    Preparing,
    Transferring,
    Completed,
    Failed(String),
    Cancelled,
}

/// An entry for a file available for download (server-side).
#[derive(Debug, Clone)]
pub struct DownloadFileEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub file_type: String,
    pub sha256: Option<String>,
}

/// A download session tracking files being pulled by a receiver.
#[derive(Debug, Clone)]
pub struct DownloadSession {
    pub session_id: String,
    pub files: HashMap<String, DownloadFileEntry>,
}
