use crate::fastswap::models::*;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

fn sha256_file(path: &PathBuf) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

pub struct TransferClient {
    client: reqwest::Client,
    progress_tracker: ProgressTracker,
}

impl TransferClient {
    pub fn new(progress_tracker: ProgressTracker) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap(),
            progress_tracker,
        }
    }

    pub async fn send_files(
        &self,
        target: &Device,
        files: Vec<PathBuf>,
        local_device: &Device,
    ) -> Result<String> {
        tracing::info!("Sending {} files to {}", files.len(), target.alias);

        let local_session_id = Uuid::new_v4().to_string();

        // Prepare file info
        let mut file_dtos = HashMap::new();
        let mut file_progresses = Vec::new();

        for path in &files {
            let metadata = tokio::fs::metadata(path).await?;
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let file_id = Uuid::new_v4().to_string();

            let sha256 = sha256_file(path).ok();

            file_dtos.insert(file_id.clone(), FileDto {
                id: file_id.clone(),
                file_name: file_name.clone(),
                size: metadata.len(),
                file_type: mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
                sha256,
                preview: None,
                metadata: None,
            });

            file_progresses.push(FileProgress::new(
                file_id,
                file_name,
                metadata.len(),
            ));
        }

        // Initialize progress tracking with local session ID
        let transfer_progress = TransferProgress::new(local_session_id.clone(), file_progresses);
        self.progress_tracker.write().await.insert(local_session_id.clone(), transfer_progress);

        // Prepare upload request
        let prepare_request = PrepareUploadRequest {
            info: RegisterRequest {
                alias: local_device.alias.clone(),
                version: local_device.version.clone(),
                device_model: Some(local_device.device_model.clone()),
                device_type: Some(local_device.device_type.clone()),
                fingerprint: local_device.fingerprint.clone(),
                port: local_device.port,
                protocol: local_device.protocol.clone(),
                download: local_device.download,
            },
            files: file_dtos.clone(),
        };

        let url = format!("http://{}:{}/api/localsend/v2/prepare-upload", target.ip, target.port);

        tracing::info!("Preparing upload to {}", url);
        let response = self.client
            .post(&url)
            .json(&prepare_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            let mut tracker = self.progress_tracker.write().await;
            if let Some(progress) = tracker.get_mut(&local_session_id) {
                for file in &mut progress.files {
                    file.status = ProgressStatus::Failed(format!("Prepare failed: {}", status));
                }
            }

            anyhow::bail!("Failed to prepare upload: {} - {}", status, body);
        }

        let prepare_response: PrepareUploadResponse = response.json().await?;
        let server_session_id = prepare_response.session_id.clone();
        tracing::info!("Upload prepared, server session: {}", server_session_id);

        // Extract encryption key if receiver supports it
        let encryption_key: Option<crate::fastswap::crypto::EncryptionKey> = prepare_response.encryption_key.as_ref().map(|k| {
            let mut key = [0u8; 32];
            if let Ok(decoded) = STANDARD.decode(k) {
                let len = decoded.len().min(32);
                key[..len].copy_from_slice(&decoded[..len]);
            }
            key
        });
        if encryption_key.is_some() {
            tracing::info!("AES-256-GCM encryption enabled for this transfer");
        }

        // Step 3: Send confirmation (ACK)
        let confirm_url = format!("http://{}:{}/api/localsend/v2/confirm-upload", target.ip, target.port);
        let confirm_request = ConfirmUploadRequest {
            session_id: server_session_id.clone(),
        };

        tracing::info!("Sending confirmation handshake...");
        let confirm_response = self.client
            .post(&confirm_url)
            .json(&confirm_request)
            .send()
            .await?;

        if !confirm_response.status().is_success() {
            let status = confirm_response.status();
            let body = confirm_response.text().await.unwrap_or_default();

            let mut tracker = self.progress_tracker.write().await;
            if let Some(progress) = tracker.get_mut(&local_session_id) {
                for file in &mut progress.files {
                    file.status = ProgressStatus::Failed(format!("Handshake failed: {}", status));
                }
            }

            anyhow::bail!("Failed to confirm upload: {} - {}", status, body);
        }

        let _confirm_resp: ConfirmUploadResponse = confirm_response.json().await?;
        tracing::info!("Three-way handshake complete, starting transfer");

        // Upload each file
        let file_ids: Vec<String> = file_dtos.keys().cloned().collect();
        for (file_path, file_id) in files.iter().zip(file_ids.iter()) {
            // Check if cancelled
            {
                let tracker = self.progress_tracker.read().await;
                if let Some(progress) = tracker.get(&local_session_id) {
                    if progress.is_cancelled {
                        tracing::info!("Transfer cancelled by user");
                        return Ok(local_session_id);
                    }
                }
            }

            let file_token = prepare_response.files.get(file_id)
                .ok_or_else(|| anyhow::anyhow!("File token not found for {}", file_id))?;

            let file_dto = file_dtos.get(file_id).unwrap();
            let enc_key = encryption_key;
            match self.upload_file(
                target,
                file_path,
                &server_session_id,
                file_id,
                file_token,
                file_dto.size,
                &file_dto.file_type,
                &local_session_id,
                enc_key,
            ).await {
                Ok(_) => {
                    let mut tracker = self.progress_tracker.write().await;
                    if let Some(progress) = tracker.get_mut(&local_session_id) {
                        progress.mark_file_completed(file_id);
                    }
                }
                Err(e) => {
                    let mut tracker = self.progress_tracker.write().await;
                    if let Some(progress) = tracker.get_mut(&local_session_id) {
                        progress.mark_file_failed(file_id, e.to_string());
                    }
                    return Err(e);
                }
            }
        }

        tracing::info!("All files sent successfully");
        Ok(local_session_id)
    }

    async fn upload_file(
        &self,
        target: &Device,
        file_path: &PathBuf,
        server_session_id: &str,
        file_id: &str,
        token: &str,
        file_size: u64,
        content_type: &str,
        local_session_id: &str,
        encryption_key: Option<crate::fastswap::crypto::EncryptionKey>,
    ) -> Result<()> {
        tracing::info!("Uploading file: {:?} ({} bytes)", file_path, file_size);

        {
            let mut tracker = self.progress_tracker.write().await;
            if let Some(progress) = tracker.get_mut(local_session_id) {
                if let Some(file) = progress.files.iter_mut().find(|f| f.file_id == file_id) {
                    file.status = ProgressStatus::Transferring;
                }
            }
        }

        let url = format!(
            "http://{}:{}/api/localsend/v2/upload?sessionId={}&fileId={}&token={}",
            target.ip, target.port, server_session_id, file_id, token
        );

        tracing::info!("Uploading to URL: {}", url);

        let file = File::open(file_path).await?;
        let mut reader = tokio::io::BufReader::new(file);

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(100);

        let tracker = self.progress_tracker.clone();
        let local_session_id = local_session_id.to_string();
        let file_id = file_id.to_string();
        let chunk_size = 64 * 1024;

        tokio::spawn(async move {
            let mut bytes_sent = 0u64;
            let mut buffer = vec![0u8; chunk_size];

            loop {
                match reader.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let plaintext = &buffer[..n];
                        bytes_sent += n as u64;

                        let chunk = if let Some(ref key) = encryption_key {
                            let encrypted = crate::fastswap::crypto::encrypt(key, plaintext);
                            bytes::Bytes::from(encrypted)
                        } else {
                            bytes::Bytes::copy_from_slice(plaintext)
                        };

                        {
                            let mut guard = tracker.write().await;
                            if let Some(progress) = guard.get_mut(&local_session_id) {
                                progress.update_file_progress(&file_id, bytes_sent);
                            }
                        }

                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }

                        if bytes_sent % (chunk_size as u64 * 10) == 0 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let body = reqwest::Body::wrap_stream(stream);

        let mut request = self.client
            .post(&url)
            .header("Content-Type", content_type);

        if encryption_key.is_none() {
            request = request.header("Content-Length", file_size.to_string());
        }

        let response = request.body(body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to upload file: {} - {}", status, body);
        }

        tracing::info!("File uploaded successfully: {:?}", file_path);
        Ok(())
    }

    pub async fn cancel_transfer(&self, session_id: &str) {
        let mut tracker = self.progress_tracker.write().await;
        if let Some(progress) = tracker.get_mut(session_id) {
            progress.cancel();
            tracing::info!("Transfer {} cancelled", session_id);
        }
    }

    /// Call POST /api/localsend/v2/prepare-download on the target device.
    /// Returns the response with session ID and available files.
    pub async fn prepare_download(
        &self,
        target: &Device,
        session_id: Option<&str>,
        pin: Option<&str>,
    ) -> Result<PrepareDownloadResponse> {
        let url = format!(
            "{}://{}:{}/api/localsend/v2/prepare-download",
            target.protocol.as_str(),
            target.ip,
            target.port,
        );

        let mut req = self.client.post(&url);
        if let Some(sid) = session_id {
            req = req.query(&[("sessionId", sid)]);
        }
        if let Some(p) = pin {
            req = req.query(&[("pin", p)]);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Prepare download failed: {} - {}", status, body);
        }

        Ok(resp.json().await?)
    }

    /// Call GET /api/localsend/v2/download to stream a file from the target.
    /// Returns the raw response bytes.
    pub async fn download(
        &self,
        target: &Device,
        session_id: &str,
        file_id: &str,
    ) -> Result<Vec<u8>> {
        let url = format!(
            "{}://{}:{}/api/localsend/v2/download?sessionId={}&fileId={}",
            target.protocol.as_str(),
            target.ip,
            target.port,
            session_id,
            file_id,
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Download failed: {} - {}", status, body);
        }

        Ok(resp.bytes().await?.to_vec())
    }
}
