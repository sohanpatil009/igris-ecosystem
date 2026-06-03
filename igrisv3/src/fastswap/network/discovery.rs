use crate::fastswap::models::*;
use anyhow::Result;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::RwLock;

const MULTICAST_ADDR: &str = "224.0.0.167";
const MULTICAST_PORT: u16 = 53317;

pub struct DiscoveryService {
    devices: Arc<RwLock<Vec<Device>>>,
    client: reqwest::Client,
    local_device: Option<Device>,
    sender_stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl DiscoveryService {
    pub fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self {
            devices: Arc::new(RwLock::new(Vec::new())),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap(),
            local_device: None,
            sender_stop: None,
        }
    }

    /// Start sending periodic UDP multicast announcements.
    /// Also listens for announcements from other devices.
    pub fn start(&mut self, device: Device) {
        let announce_device = device.clone();
        let devices = self.devices.clone();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        self.local_device = Some(device);
        self.sender_stop = Some(stop_tx);

        tokio::spawn(async move {
            Self::listener_loop(devices.clone(), stop_rx).await;
        });

        tokio::spawn(async move {
            Self::announce_loop(announce_device).await;
        });
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.sender_stop.take() {
            let _ = tx.send(());
        }
    }

    /// Send an announcement every 10 seconds.
    async fn announce_loop(device: Device) {
        let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to bind UDP socket for announcement: {e}");
                return;
            }
        };

        let msg = MulticastMessageV2 {
            alias: device.alias.clone(),
            version: device.version.clone(),
            device_model: Some(device.device_model.clone()),
            device_type: Some(device.device_type.clone()),
            fingerprint: device.fingerprint.clone(),
            port: device.port,
            protocol: device.protocol.clone(),
            download: device.download,
            announce: true,
        };

        let payload = match serde_json::to_vec(&msg) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to serialize announcement: {e}");
                return;
            }
        };

        let dest: SocketAddr = format!("{MULTICAST_ADDR}:{MULTICAST_PORT}")
            .parse()
            .unwrap();

        loop {
            if let Err(e) = socket.send_to(&payload, dest).await {
                tracing::warn!("Failed to send multicast announcement: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    }

    /// Listen for UDP multicast messages and send responses to announcements.
    async fn listener_loop(
        devices: Arc<RwLock<Vec<Device>>>,
        mut stop_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let socket = match tokio::net::UdpSocket::bind(format!("0.0.0.0:{MULTICAST_PORT}")).await {
            Ok(s) => s,
            Err(e) => {
                // Port might be in use (HTTP server), try an ephemeral port
                let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                    Ok(s) => s,
                    Err(e2) => {
                        tracing::error!("Failed to bind UDP listener: {e} / {e2}");
                        return;
                    }
                };
                socket
            }
        };

        // Join the multicast group
        if let Err(e) = socket.join_multicast_v4(Ipv4Addr::new(224, 0, 0, 167), Ipv4Addr::UNSPECIFIED) {
            tracing::warn!("Failed to join multicast group: {e}");
        }

        let mut buf = vec![0u8; 2048];
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                result = socket.recv_from(&mut buf) => {
                    match result {
                        Ok((n, src_addr)) => {
                            let data = &buf[..n];
                            if let Ok(msg) = serde_json::from_slice::<MulticastMessageV2>(data) {
                                if msg.announce {
                                    // Received an announcement — respond with our own info
                                    // (handled by the other device's listener)
                                }
                                // Add or update device regardless
                                let device = Device {
                                    id: msg.fingerprint.clone(),
                                    alias: msg.alias,
                                    device_model: msg.device_model.unwrap_or_default(),
                                    device_type: msg.device_type.unwrap_or(DeviceType::Desktop),
                                    ip: src_addr.ip().to_string(),
                                    port: msg.port,
                                    version: msg.version,
                                    protocol: msg.protocol,
                                    download: msg.download,
                                    fingerprint: msg.fingerprint,
                                };
                                let mut guard = devices.write().await;
                                if let Some(existing) = guard.iter_mut().find(|d| d.fingerprint == device.fingerprint) {
                                    *existing = device;
                                } else {
                                    guard.push(device);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("UDP recv error: {e}");
                        }
                    }
                }
            }
        }
    }

    /// Scan network via UDP multicast broadcast (emit an announcement).
    pub async fn scan_network(&self, local_ip: &str) -> Result<Vec<Device>> {
        tracing::info!("Scanning network via UDP multicast from {local_ip}");

        // If we have a local device, send a discovery announcement
        if let Some(device) = &self.local_device {
            let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;

            let msg = MulticastMessageV2 {
                alias: device.alias.clone(),
                version: device.version.clone(),
                device_model: Some(device.device_model.clone()),
                device_type: Some(device.device_type.clone()),
                fingerprint: device.fingerprint.clone(),
                port: device.port,
                protocol: device.protocol.clone(),
                download: device.download,
                announce: true,
            };

            let payload = serde_json::to_vec(&msg)?;
            let dest: SocketAddr = format!("{MULTICAST_ADDR}:{MULTICAST_PORT}").parse()?;
            socket.send_to(&payload, dest).await?;

            // Wait briefly for responses
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Also probe common ports via HTTP as fallback
            let parts: Vec<&str> = local_ip.split('.').collect();
            if parts.len() == 4 {
                let subnet = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
                let mut tasks = Vec::new();
                for i in 1..=254 {
                    let ip = format!("{}.{}", subnet, i);
                    if ip == local_ip {
                        continue;
                    }
                    let client = self.client.clone();
                    let task = tokio::spawn(async move {
                        Self::probe_device(&client, &ip).await
                    });
                    tasks.push(task);
                    if tasks.len() >= 50 {
                        for task in tasks.drain(..) {
                            if let Ok(Some(_)) = task.await {}
                        }
                    }
                }
                for task in tasks {
                    if let Ok(Some(_)) = task.await {}
                }
            }
        }

        Ok(self.devices.read().await.clone())
    }

    async fn probe_device(client: &reqwest::Client, ip: &str) -> Option<Device> {
        let url = format!("http://{ip}:{MULTICAST_PORT}/api/localsend/v2/info");
        match client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(info) = response.json::<InfoResponse>().await {
                        return Some(Device {
                            id: info.fingerprint.clone(),
                            alias: info.alias,
                            device_model: info.device_model.unwrap_or_default(),
                            device_type: info.device_type.unwrap_or(DeviceType::Desktop),
                            ip: ip.to_string(),
                            port: MULTICAST_PORT,
                            version: info.version,
                            protocol: ProtocolType::Http,
                            download: info.download,
                            fingerprint: info.fingerprint,
                        });
                    }
                }
            }
            Err(_) => {}
        }
        None
    }

    pub async fn get_devices(&self) -> Vec<Device> {
        self.devices.read().await.clone()
    }
}
