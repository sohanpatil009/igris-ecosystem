use crate::fastswap::models::*;
use crate::fastswap::token;
use crate::fastswap::tls;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use futures::StreamExt;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tower::Service;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct ServerState {
    pub local_device: Device,
    pub signing_key: Arc<token::SigningTokenKey>,
    pub sessions: Arc<RwLock<Vec<TransferState>>>,
    pub download_sessions: Arc<RwLock<Vec<DownloadSession>>>,
    pub pin: Option<String>,
}

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/api/localsend/v2/info", get(info_handler))
        .route("/api/localsend/v2/register", post(register_handler))
        .route("/api/localsend/v2/prepare-upload", post(prepare_upload_handler))
        .route("/api/localsend/v2/confirm-upload", post(confirm_upload_handler))
        .route("/api/localsend/v2/upload", post(upload_handler))
        .route("/api/localsend/v2/prepare-download", post(prepare_download_handler))
        .route("/api/localsend/v2/download", get(download_handler))
        .route("/api/localsend/v2/cancel", post(cancel_handler))
        .layer(
            tower::ServiceBuilder::new()
                .layer(axum::extract::DefaultBodyLimit::max(10usize * 1024 * 1024 * 1024))
        )
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────────────

async fn info_handler(State(state): State<ServerState>) -> Json<InfoResponse> {
    Json(InfoResponse {
        alias: state.local_device.alias.clone(),
        version: state.local_device.version.clone(),
        device_model: Some(state.local_device.device_model.clone()),
        device_type: Some(state.local_device.device_type.clone()),
        fingerprint: state.local_device.fingerprint.clone(),
        download: state.local_device.download,
    })
}

async fn register_handler(
    State(state): State<ServerState>,
    Json(_request): Json<RegisterRequest>,
) -> Json<RegisterResponse> {
    Json(RegisterResponse {
        alias: state.local_device.alias.clone(),
        version: state.local_device.version.clone(),
        device_model: Some(state.local_device.device_model.clone()),
        device_type: Some(state.local_device.device_type.clone()),
        fingerprint: state.local_device.fingerprint.clone(),
        download: state.local_device.download,
    })
}

#[derive(Deserialize)]
struct PrepareUploadQuery {
    pin: Option<String>,
}

async fn prepare_upload_handler(
    State(state): State<ServerState>,
    Query(query): Query<PrepareUploadQuery>,
    Json(request): Json<PrepareUploadRequest>,
) -> Result<Json<PrepareUploadResponse>, StatusCode> {
    if let Some(ref expected_pin) = state.pin {
        match query.pin {
            Some(ref pin) if pin == expected_pin => {}
            _ => return Err(StatusCode::UNAUTHORIZED),
        }
    }

    let session_id = Uuid::new_v4().to_string();
    let mut file_tokens = std::collections::HashMap::new();

    for (file_id, _file) in &request.files {
        file_tokens.insert(file_id.clone(), Uuid::new_v4().to_string());
    }

    let total_size: u64 = request.files.values().map(|f| f.size).sum();

    let pending = crate::fastswap::PendingTransfer {
        session_id: session_id.clone(),
        sender_name: request.info.alias.clone(),
        sender_device: request.info.device_model.clone().unwrap_or_default(),
        file_count: request.files.len(),
        total_size,
        files: request.files.values().map(|f| f.file_name.clone()).collect(),
    };
    crate::fastswap::add_pending_transfer(pending).await;

    let raw_key = crate::fastswap::crypto::generate_key();
    let encryption_key_b64 = STANDARD.encode(raw_key);

    let transfer_state = TransferState {
        session_id: session_id.clone(),
        files: request.files.iter().map(|(file_id, file)| FileTransfer {
            id: file_id.clone(),
            name: file.file_name.clone(),
            path: std::path::PathBuf::from(&file.file_name),
            size: file.size,
            transferred: 0,
            token: file_tokens.get(file_id).cloned(),
        }).collect(),
        total_size,
        transferred: 0,
        status: TransferStatus::Preparing,
        confirmed: false,
        encryption_key: Some(encryption_key_b64.clone()),
    };
    state.sessions.write().await.push(transfer_state);

    Ok(Json(PrepareUploadResponse {
        session_id,
        files: file_tokens,
        encryption_key: Some(encryption_key_b64),
    }))
}

async fn confirm_upload_handler(
    State(state): State<ServerState>,
    Json(request): Json<ConfirmUploadRequest>,
) -> Result<Json<ConfirmUploadResponse>, StatusCode> {
    let max_wait = std::time::Duration::from_secs(60);
    let poll = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();

    loop {
        if crate::fastswap::is_transfer_approved(&request.session_id).await {
            break;
        }

        let pending = crate::fastswap::get_pending_transfers().await;
        let still_pending = pending.iter().any(|t| t.session_id == request.session_id);
        if !still_pending && !crate::fastswap::is_transfer_approved(&request.session_id).await {
            return Err(StatusCode::FORBIDDEN);
        }

        if start.elapsed() > max_wait {
            crate::fastswap::deny_transfer(&request.session_id).await;
            return Err(StatusCode::REQUEST_TIMEOUT);
        }

        tokio::time::sleep(poll).await;
    }

    let mut sessions = state.sessions.write().await;
    let session = sessions.iter_mut()
        .find(|s| s.session_id == request.session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    session.confirmed = true;
    session.status = TransferStatus::Transferring;

    let file_progresses: Vec<FileProgress> = session.files.iter().map(|f| {
        FileProgress::new(f.id.clone(), f.name.clone(), f.size)
    }).collect();

    let transfer_progress = TransferProgress::new(request.session_id.clone(), file_progresses);
    let tracker = crate::fastswap::get_progress_tracker();
    tracker.write().await.insert(request.session_id.clone(), transfer_progress);

    Ok(Json(ConfirmUploadResponse {
        status: "ready".to_string(),
    }))
}

#[derive(Deserialize)]
struct UploadQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "fileId")]
    file_id: String,
    token: String,
}

async fn upload_handler(
    State(state): State<ServerState>,
    Query(query): Query<UploadQuery>,
    req: axum::extract::Request,
) -> Result<StatusCode, StatusCode> {
    let (_, body) = req.into_parts();
    let mut stream = BodyStream::new(body);

    let download_dir = dirs::download_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let sessions = state.sessions.read().await;
    let session = sessions.iter()
        .find(|s| s.session_id == query.session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.confirmed {
        return Err(StatusCode::PRECONDITION_FAILED);
    }

    let file = session.files.iter()
        .find(|f| f.id == query.file_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(expected_token) = &file.token {
        if expected_token != &query.token {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let expected_size = file.size;
    let encryption_key: Option<crate::fastswap::crypto::EncryptionKey> = session.encryption_key.as_ref().map(|k| {
        let mut key = [0u8; 32];
        if let Ok(decoded) = STANDARD.decode(k) {
            let len = decoded.len().min(32);
            key[..len].copy_from_slice(&decoded[..len]);
        }
        key
    });

    let file_name = std::path::Path::new(&file.name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let file_path = download_dir.join(&file_name);
    let tmp_path = file_path.with_extension(format!("{}.tmp", query.file_id));

    drop(sessions);

    let mut tmp_file = tokio::fs::File::create(&tmp_path).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut bytes_received = 0u64;
    while let Some(frame_result) = stream.next().await {
        let frame = frame_result.map_err(|_| StatusCode::BAD_REQUEST)?;
        if let Some(data) = frame.data_ref() {
            let plaintext = if let Some(ref key) = encryption_key {
                crate::fastswap::crypto::decrypt(key, data)
                    .map_err(|_| StatusCode::BAD_REQUEST)?
            } else {
                data.to_vec()
            };

            bytes_received += plaintext.len() as u64;

            {
                let tracker = crate::fastswap::get_progress_tracker();
                let mut guard = tracker.write().await;
                if let Some(progress) = guard.get_mut(&query.session_id) {
                    progress.update_file_progress(&query.file_id, bytes_received);
                }
            }

            tmp_file.write_all(&plaintext).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    tmp_file.flush().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    drop(tmp_file);

    if encryption_key.is_none() && bytes_received != expected_size {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(StatusCode::BAD_REQUEST);
    }

    let _ = tokio::fs::remove_file(&file_path).await;
    tokio::fs::rename(&tmp_path, &file_path).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    {
        let tracker = crate::fastswap::get_progress_tracker();
        let mut guard = tracker.write().await;
        if let Some(progress) = guard.get_mut(&query.session_id) {
            progress.mark_file_completed(&query.file_id);
        }
    }

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct PrepareDownloadQuery {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    pin: Option<String>,
}

#[derive(Deserialize)]
struct DownloadQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "fileId")]
    file_id: String,
}

async fn prepare_download_handler(
    State(state): State<ServerState>,
    Query(query): Query<PrepareDownloadQuery>,
) -> Result<Json<PrepareDownloadResponse>, StatusCode> {
    if let Some(ref expected_pin) = state.pin {
        match query.pin {
            Some(ref pin) if pin == expected_pin => {}
            _ => return Err(StatusCode::UNAUTHORIZED),
        }
    }

    let session_id = Uuid::new_v4().to_string();

    // Snapshot currently registered download files
    let download_files = crate::fastswap::get_download_files().await;
    let mut files_map = std::collections::HashMap::new();
    for entry in &download_files {
        files_map.insert(entry.id.clone(), FileDto {
            id: entry.id.clone(),
            file_name: entry.name.clone(),
            size: entry.size,
            file_type: entry.file_type.clone(),
            sha256: entry.sha256.clone(),
            preview: None,
            metadata: None,
        });
    }

    // Persist the session so download_handler can look up paths
    {
        let mut sessions = state.download_sessions.write().await;
        sessions.push(DownloadSession {
            session_id: session_id.clone(),
            files: download_files.into_iter().map(|e| (e.id.clone(), e)).collect(),
        });
    }

    Ok(Json(PrepareDownloadResponse {
        info: InfoResponse {
            alias: state.local_device.alias.clone(),
            version: state.local_device.version.clone(),
            device_model: Some(state.local_device.device_model.clone()),
            device_type: Some(state.local_device.device_type.clone()),
            fingerprint: state.local_device.fingerprint.clone(),
            download: state.local_device.download,
        },
        session_id,
        files: files_map,
    }))
}

async fn download_handler(
    State(state): State<ServerState>,
    Query(query): Query<DownloadQuery>,
) -> Result<(axum::http::HeaderMap, Vec<u8>), StatusCode> {
    let entry = {
        let sessions = state.download_sessions.read().await;
        sessions
            .iter()
            .find(|s| s.session_id == query.session_id)
            .and_then(|s| s.files.get(&query.file_id))
            .cloned()
    };

    let entry = entry.ok_or(StatusCode::NOT_FOUND)?;

    let bytes = tokio::fs::read(&entry.path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        entry.file_type.parse().unwrap(),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", entry.name).parse().unwrap(),
    );

    Ok((headers, bytes))
}

#[derive(Deserialize)]
struct CancelQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
}

async fn cancel_handler(
    State(state): State<ServerState>,
    Query(query): Query<CancelQuery>,
) -> StatusCode {
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.iter_mut().find(|s| s.session_id == query.session_id) {
        session.status = TransferStatus::Cancelled;
    }
    StatusCode::OK
}

// ── Server startup ────────────────────────────────────────────────────

/// Serve an axum Router over a single TLS connection.
async fn serve_tls_connection(
    router: Router,
    tls_stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
) {
    let svc = service_fn(move |req: hyper::Request<Incoming>| {
        let mut router = router.clone();
        async move {
            match router.call(req).await {
                Ok(resp) => Ok::<_, Infallible>(resp),
                Err(e) => match e {},
            }
        }
    });

    let io = TokioIo::new(tls_stream);
    let _ = http1::Builder::new()
        .serve_connection(io, svc)
        .await;
}

/// Start an HTTP server on the given port.
pub async fn start_http_server(
    port: u16,
    local_device: Device,
    pin: Option<String>,
) -> Result<(u16, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
    let signing_key = token::generate_key();
    let state = ServerState {
        local_device: local_device.clone(),
        signing_key: Arc::new(signing_key),
        sessions: Arc::new(RwLock::new(Vec::new())),
        download_sessions: Arc::new(RwLock::new(Vec::new())),
        pin,
    };

    let app = create_router(state);

    for try_port in port..port + 10 {
        let addr = format!("0.0.0.0:{}", try_port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                tracing::info!("FastSwap HTTP server on port {}", try_port);
                let handle = tokio::spawn(async move {
                    if let Err(e) = axum::serve(listener, app).await {
                        tracing::error!("HTTP server error: {}", e);
                    }
                });
                return Ok((try_port, handle));
            }
            Err(_) => {
                if try_port == port {
                    tracing::warn!("Port {} in use, alternatives...", port);
                }
                continue;
            }
        }
    }

    Err(format!("No free port in {}-{}", port, port + 9).into())
}

/// Start an HTTPS server on the given port with a self-signed certificate.
pub async fn start_tls_server(
    port: u16,
    local_device: Device,
    pin: Option<String>,
) -> Result<(u16, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
    let cert = tls::generate_self_signed_cert()?;
    let fingerprint = tls::cert_fingerprint(&cert.cert_pem)?;

    let server_config = tls::create_server_config(&cert.cert_pem, &cert.key_pem)?;
    let acceptor = std::sync::Arc::new(tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config)));

    let signing_key = token::generate_key();
    let state = ServerState {
        local_device: Device {
            fingerprint: fingerprint.clone(),
            protocol: ProtocolType::Https,
            ..local_device.clone()
        },
        signing_key: Arc::new(signing_key),
        sessions: Arc::new(RwLock::new(Vec::new())),
        download_sessions: Arc::new(RwLock::new(Vec::new())),
        pin,
    };

    let app = Arc::new(create_router(state));

    for try_port in port..port + 10 {
        let addr = format!("0.0.0.0:{}", try_port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                let acceptor = acceptor.clone();
                tracing::info!("FastSwap TLS server on port {}", try_port);
                tracing::info!("Certificate fingerprint: {}", fingerprint);
                let handle = tokio::spawn(async move {
                    loop {
                        let (stream, _) = match listener.accept().await {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                                continue;
                            }
                        };
                        let acceptor = acceptor.clone();
                        let app = app.clone();
                        tokio::spawn(async move {
                            let tls_stream = match acceptor.accept(stream).await {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::warn!("TLS accept error: {e}");
                                    return;
                                }
                            };
                            serve_tls_connection((*app).clone(), tls_stream).await;
                        });
                    }
                });
                return Ok((try_port, handle));
            }
            Err(_) => {
                if try_port == port {
                    tracing::warn!("Port {} in use, alternatives...", port);
                }
                continue;
            }
        }
    }

    Err(format!("No free port in {}-{}", port, port + 9).into())
}

/// Start a server on the given port, using HTTP by default.
pub async fn start_server(
    port: u16,
    local_device: Device,
) -> Result<(u16, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
    start_http_server(port, local_device, None).await
}
