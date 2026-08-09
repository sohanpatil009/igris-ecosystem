#[cfg(feature = "with_riva")]
use tonic::transport::{Channel, Endpoint};
#[cfg(feature = "with_riva")]
use tonic::metadata::MetadataValue;

#[cfg(feature = "with_riva")]
fn init_rustls_crypto() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        if provider.install_default().is_err() {
            println!("[Parakeet STT] rustls CryptoProvider already initialized by another crate");
        } else {
            println!("[Parakeet STT] rustls CryptoProvider initialized (aws-lc-rs)");
        }
    });
}

#[cfg(feature = "with_riva")]
const SAMPLE_RATE: u32 = 16000;
#[cfg(feature = "with_riva")]
const FUNCTION_ID: &str = "d3fe9151-442b-4204-a70d-5fcc597fd610";
#[cfg(feature = "with_riva")]
const GRPC_ENDPOINT: &str = "https://grpc.nvcf.nvidia.com:443";

#[cfg(feature = "with_riva")]
mod generated {
    tonic::include_proto!("nvidia.riva");
    tonic::include_proto!("nvidia.riva.asr");
}

#[cfg(feature = "with_riva")]
pub use generated::{AudioEncoding, RequestId, RecognizeRequest, RecognitionConfig};
#[cfg(feature = "with_riva")]
pub use generated::riva_speech_recognition_client::RivaSpeechRecognitionClient;

#[cfg(feature = "with_riva")]
pub struct OnlineStt {
    client: RivaSpeechRecognitionClient<Channel>,
    api_key: String,
}

#[cfg(feature = "with_riva")]
impl OnlineStt {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        init_rustls_crypto();
        println!("[Parakeet STT] Initializing gRPC client...");

        let api_key = std::env::var("NVIDIA_API_KEY")
            .map_err(|e| {
                println!("[Parakeet STT] ERROR: NVIDIA_API_KEY not set: {}", e);
                "NVIDIA_API_KEY not set in .env"
            })?;

        println!("[Parakeet STT] Connecting to {} ...", GRPC_ENDPOINT);

        let endpoint = Endpoint::new(GRPC_ENDPOINT.to_string())?;
        let channel = endpoint
            .connect_timeout(std::time::Duration::from_secs(10))
            .connect()
            .await
            .map_err(|e| {
                println!("[Parakeet STT] ERROR connecting gRPC channel: {:#}", e);
                e
            })?;

        println!("[Parakeet STT] gRPC channel connected");

        let client = RivaSpeechRecognitionClient::new(channel);

        Ok(Self { client, api_key })
    }

    pub async fn transcribe(&mut self, audio_samples: &[f32]) -> Result<String, Box<dyn std::error::Error>> {
        let duration_ms = (audio_samples.len() as f64 / SAMPLE_RATE as f64) * 1000.0;
        println!("[Parakeet STT] Transcribing {:.0}ms of audio ({} samples)", duration_ms, audio_samples.len());

        let audio_bytes: Vec<u8> = audio_samples
            .iter()
            .flat_map(|&s| {
                let sample = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                sample.to_le_bytes().to_vec()
            })
            .collect();

        println!("[Parakeet STT] Raw PCM: {} bytes ({} samples)", audio_bytes.len(), audio_samples.len());

        let request = RecognizeRequest {
            config: Some(RecognitionConfig {
                encoding: AudioEncoding::LinearPcm as i32,
                sample_rate_hertz: SAMPLE_RATE as i32,
                language_code: "en-US".to_string(),
                audio_channel_count: 1,
                enable_automatic_punctuation: true,
                model: String::new(),
                max_alternatives: 1,
                profanity_filter: false,
                speech_contexts: vec![],
                enable_word_time_offsets: false,
                enable_separate_recognition_per_channel: false,
                verbatim_transcripts: true,
                diarization_config: None,
                custom_configuration: std::collections::HashMap::new(),
                ..Default::default()
            }),
            audio: audio_bytes,
            id: None,
        };

        println!("[Parakeet STT] gRPC Recognize via {} (function-id: {})", GRPC_ENDPOINT, FUNCTION_ID);

        let mut tonic_req = tonic::Request::new(request);
        tonic_req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(&format!("Bearer {}", self.api_key))?,
        );
        tonic_req.metadata_mut().insert(
            "function-id",
            MetadataValue::try_from(FUNCTION_ID)?,
        );

        let response = self.client.recognize(tonic_req).await?;
        let response = response.into_inner();

        println!("[Parakeet STT] Got {} result(s)", response.results.len());

        if let Some(result) = response.results.first() {
            if let Some(alt) = result.alternatives.first() {
                let transcript = alt.transcript.trim().to_string();
                println!("[Parakeet STT] Transcription: \"{}\"", transcript);
                return Ok(transcript);
            }
        }

        println!("[Parakeet STT] No transcription in response");
        Ok(String::new())
    }
}

#[cfg(feature = "with_riva")]
pub async fn transcribe_online(audio_samples: &[f32]) -> Result<String, Box<dyn std::error::Error>> {
    let mut stt = OnlineStt::new().await?;
    stt.transcribe(audio_samples).await
}

#[cfg(not(feature = "with_riva"))]
pub struct OnlineStt;

#[cfg(not(feature = "with_riva"))]
impl OnlineStt {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Err(Box::<dyn std::error::Error>::from("riva proto not available: build with feature 'with_riva' and provide proto files"))
    }

    pub async fn transcribe(&mut self, _audio_samples: &[f32]) -> Result<String, Box<dyn std::error::Error>> {
        Err(Box::<dyn std::error::Error>::from("riva proto not available: build with feature 'with_riva' and provide proto files"))
    }
}

#[cfg(not(feature = "with_riva"))]
pub async fn transcribe_online(_audio_samples: &[f32]) -> Result<String, Box<dyn std::error::Error>> {
    Err(Box::<dyn std::error::Error>::from("riva proto not available: build with feature 'with_riva' and provide proto files"))
}

#[cfg(test)]
mod tests {
    // tests are only relevant when with_riva is enabled
}
