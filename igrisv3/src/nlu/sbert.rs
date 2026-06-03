use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub const EMBEDDING_DIM: usize = 384;

#[derive(Debug, Clone, Deserialize)]
struct VocabEntry {
    id: u32,
}

pub struct SbertTokenizer {
    vocab: HashMap<String, u32>,
    id_to_token: HashMap<u32, String>,
    unk_token_id: u32,
    cls_token_id: u32,
    sep_token_id: u32,
    pad_token_id: u32,
    max_length: usize,
}

impl SbertTokenizer {
    pub fn load(model_path: &PathBuf) -> Result<Self> {
        let tokenizer_path = model_path.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(anyhow!("Tokenizer not found: {:?}", tokenizer_path));
        }
        let file = File::open(&tokenizer_path)?;
        let reader = BufReader::new(file);
        let json: serde_json::Value = serde_json::from_reader(reader)?;

        let mut vocab = HashMap::new();
        let mut id_to_token = HashMap::new();
        if let Some(model) = json.get("model") {
            if let Some(vocab_obj) = model.get("vocab") {
                if let Some(vocab_map) = vocab_obj.as_object() {
                    for (token, id) in vocab_map {
                        if let Some(id_num) = id.as_u64() {
                            vocab.insert(token.clone(), id_num as u32);
                            id_to_token.insert(id_num as u32, token.clone());
                        }
                    }
                }
            }
        }

        let unk_token_id = *vocab.get("[UNK]").unwrap_or(&0);
        let cls_token_id = *vocab.get("[CLS]").unwrap_or(&101);
        let sep_token_id = *vocab.get("[SEP]").unwrap_or(&102);
        let pad_token_id = *vocab.get("[PAD]").unwrap_or(&0);

        Ok(Self { vocab, id_to_token, unk_token_id, cls_token_id, sep_token_id, pad_token_id, max_length: 128 })
    }

    pub fn encode(&self, text: &str) -> Vec<u32> {
        let mut tokens = vec![self.cls_token_id];
        let text_lower = text.to_lowercase();
        for word in text_lower.split_whitespace() {
            if let Some(&id) = self.vocab.get(word) {
                tokens.push(id);
            } else {
                tokens.extend(self.wordpiece_tokenize(word));
            }
            if tokens.len() >= self.max_length - 1 {
                break;
            }
        }
        tokens.push(self.sep_token_id);
        while tokens.len() < self.max_length {
            tokens.push(self.pad_token_id);
        }
        tokens.truncate(self.max_length);
        tokens
    }

    fn wordpiece_tokenize(&self, word: &str) -> Vec<u32> {
        let mut tokens = Vec::new();
        let mut start = 0;
        while start < word.len() {
            let mut end = word.len();
            let mut found = false;
            while start < end {
                let substr = if start == 0 {
                    word[start..end].to_string()
                } else {
                    format!("##{}", &word[start..end])
                };
                if let Some(&id) = self.vocab.get(&substr) {
                    tokens.push(id);
                    found = true;
                    start = end;
                    break;
                }
                end -= 1;
            }
            if !found {
                tokens.push(self.unk_token_id);
                start += 1;
            }
        }
        tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentEmbedding {
    pub intent_name: String,
    pub phrase: String,
    pub embedding: Vec<f32>,
}

pub struct SbertEngine {
    tokenizer: Option<SbertTokenizer>,
    intent_embeddings: Vec<IntentEmbedding>,
    session: Option<Session>,
    model_loaded: bool,
    embeddings_cache: HashMap<String, Vec<f32>>,
}

impl SbertEngine {
    pub fn new() -> Self {
        Self { tokenizer: None, intent_embeddings: Vec::new(), session: None, model_loaded: false, embeddings_cache: HashMap::new() }
    }

    pub fn initialize(&mut self) -> Result<()> {
        let model_path = PathBuf::from("./pkg/models/sbert");
        if !model_path.exists() {
            println!("SBERT model dir not found, using hash fallback");
            self.load_precomputed_embeddings();
            return Ok(());
        }

        let onnx_path = model_path.join("model.onnx");
        if onnx_path.exists() {
            match Session::builder()?.commit_from_file(&onnx_path) {
                Ok(session) => {
                    self.session = Some(session);
                    println!("ONNX SBERT model loaded");
                }
                Err(e) => {
                    println!("ONNX load failed: {}, using fallback", e);
                }
            }
        } else {
            println!("model.onnx not found, using hash fallback");
        }

        match SbertTokenizer::load(&model_path) {
            Ok(tokenizer) => {
                self.tokenizer = Some(tokenizer);
                println!("SBERT tokenizer loaded");
            }
            Err(e) => {
                println!("Tokenizer load failed: {}, using fallback", e);
            }
        }

        self.load_precomputed_embeddings();
        if self.session.is_some() {
            self.recompute_embeddings_with_onnx();
        }
        self.model_loaded = true;
        println!("SBERT engine ready with {} intents", self.intent_embeddings.len());
        Ok(())
    }

    fn load_precomputed_embeddings(&mut self) {
        let phrases = vec![
            ("open_app", "open chrome"),
            ("open_app", "launch firefox"),
            ("open_app", "start notepad"),
            ("open_app", "run calculator"),
            ("open_app", "open browser"),
            ("open_app", "launch application"),
            ("open_app", "start program"),
            ("open_app", "open spotify"),
            ("open_app", "launch discord"),
            ("open_app", "open vscode"),
            ("close_app", "close chrome"),
            ("close_app", "quit firefox"),
            ("close_app", "exit notepad"),
            ("close_app", "terminate application"),
            ("close_app", "kill process"),
            ("close_app", "shut down program"),
            ("close_app", "close all apps"),
            ("system_control", "shutdown computer"),
            ("system_control", "restart system"),
            ("system_control", "lock screen"),
            ("system_control", "sleep mode"),
            ("system_control", "hibernate"),
            ("system_control", "increase volume"),
            ("system_control", "decrease volume"),
            ("system_control", "set volume to fifty"),
            ("system_control", "mute audio"),
            ("system_control", "unmute sound"),
            ("system_control", "turn on wifi"),
            ("system_control", "disable bluetooth"),
            ("system_control", "increase brightness"),
            ("system_control", "lower brightness"),
            ("file_operation", "create file"),
            ("file_operation", "delete document"),
            ("file_operation", "copy file"),
            ("file_operation", "move folder"),
            ("file_operation", "rename file"),
            ("file_operation", "open folder"),
            ("file_operation", "search files"),
            ("camera_control", "open camera"),
            ("camera_control", "camera"),
            ("camera_control", "start camera"),
            ("camera_control", "launch camera"),
            ("camera_control", "camera mode"),
            ("camera_control", "take photo"),
            ("camera_control", "take a photo"),
            ("camera_control", "capture image"),
            ("camera_control", "take picture"),
            ("camera_control", "snap photo"),
            ("camera_control", "record video"),
            ("camera_control", "start recording"),
            ("camera_control", "stop recording"),
            ("web_search", "search for"),
            ("web_search", "google something"),
            ("web_search", "look up information"),
            ("web_search", "find on internet"),
            ("web_search", "what is"),
            ("web_search", "who is"),
            ("web_search", "where is"),
            ("web_search", "how to"),
            ("assistant_control", "go to sleep"),
            ("assistant_control", "standby mode"),
            ("assistant_control", "exit assistant"),
            ("assistant_control", "quit igris"),
            ("assistant_control", "goodbye"),
            ("assistant_control", "shut down assistant"),
            ("greeting", "hello"),
            ("greeting", "hi there"),
            ("greeting", "good morning"),
            ("greeting", "hey igris"),
        ];
        for (intent, phrase) in phrases {
            let embedding = self.embed_fallback(phrase);
            self.intent_embeddings.push(IntentEmbedding {
                intent_name: intent.to_string(),
                phrase: phrase.to_string(),
                embedding,
            });
        }
    }

    fn recompute_embeddings_with_onnx(&mut self) {
        let phrases: Vec<String> = self.intent_embeddings.iter().map(|e| e.phrase.clone()).collect();
        let mut count = 0;
        for (i, phrase) in phrases.iter().enumerate() {
            if let Some(real) = self.embed_onnx(phrase) {
                if let Some(emb) = self.intent_embeddings.get_mut(i) {
                    emb.embedding = real;
                    count += 1;
                }
            }
        }
        println!("Recomputed {} intent embeddings with ONNX", count);
    }

    fn embed_onnx(&mut self, text: &str) -> Option<Vec<f32>> {
        let token_ids = self.tokenizer.as_ref()?.encode(text);
        let seq_len = token_ids.len();
        let pad_id = self.tokenizer.as_ref()?.pad_token_id as u32;

        let input_ids: Vec<i64> = token_ids.iter().map(|&id| id as i64).collect();
        let mask: Vec<i64> = token_ids.iter()
            .map(|&id| if id == pad_id { 0 } else { 1 })
            .collect();
        let type_ids: Vec<i64> = vec![0i64; seq_len];

        let shape: Vec<i64> = vec![1i64, seq_len as i64];
        let input_tensor = Value::from_array((&shape[..], input_ids)).ok()?;
        let mask_tensor = Value::from_array((&shape[..], mask)).ok()?;
        let type_tensor = Value::from_array((&shape[..], type_ids)).ok()?;

        let session = self.session.as_mut()?;
        let outputs = session.run(ort::inputs! {
            "input_ids" => input_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => type_tensor,
        }).ok()?;

        let preferred = ["sentence_embedding", "embedding", "last_hidden_state", "pooler_output", "output"];
        for name in &preferred {
            if let Some(val) = outputs.get(*name) {
                if let Ok(tensor) = val.try_extract_tensor::<f32>() {
                    let flat: Vec<f32> = tensor.1.to_vec();
                    if !flat.is_empty() {
                        return Some(flat);
                    }
                }
            }
        }
        None
    }

    fn embed_fallback(&self, text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0f32; EMBEDDING_DIM];
        let text_lower = text.to_lowercase();
        let chars: Vec<char> = text_lower.chars().collect();
        for i in 0..chars.len().saturating_sub(2) {
            let trigram: String = chars[i..i + 3].iter().collect();
            let hash = Self::hash_string(&trigram);
            let idx = (hash as usize) % EMBEDDING_DIM;
            embedding[idx] += 1.0;
        }
        for word in text_lower.split_whitespace() {
            let hash = Self::hash_string(word);
            let idx = (hash as usize) % EMBEDDING_DIM;
            embedding[idx] += 2.0;
        }
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }
        embedding
    }

    fn hash_string(s: &str) -> u64 {
        let mut hash: u64 = 5381;
        for c in s.chars() {
            hash = hash.wrapping_mul(33).wrapping_add(c as u64);
        }
        hash
    }

    pub fn embed(&mut self, text: &str) -> Vec<f32> {
        if let Some(cached) = self.embeddings_cache.get(text) {
            return cached.clone();
        }
        let embedding = if self.session.is_some() {
            self.embed_onnx(text).unwrap_or_else(|| self.embed_fallback(text))
        } else {
            self.embed_fallback(text)
        };
        if self.embeddings_cache.len() < 1000 {
            self.embeddings_cache.insert(text.to_string(), embedding.clone());
        }
        embedding
    }

    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }

    pub fn find_intent(&mut self, text: &str) -> Option<(String, f32)> {
        let input_embedding = self.embed(text);
        let mut best_intent = String::new();
        let mut best_score = 0.0f32;
        for intent_emb in &self.intent_embeddings {
            let similarity = Self::cosine_similarity(&input_embedding, &intent_emb.embedding);
            if similarity > best_score {
                best_score = similarity;
                best_intent = intent_emb.intent_name.clone();
            }
        }
        if best_score > 0.3 {
            Some((best_intent, best_score))
        } else {
            None
        }
    }

    pub fn find_top_intents(&mut self, text: &str, k: usize) -> Vec<(String, f32)> {
        let input_embedding = self.embed(text);
        let mut scores: Vec<(String, f32)> = self.intent_embeddings
            .iter()
            .map(|intent_emb| {
                let sim = Self::cosine_similarity(&input_embedding, &intent_emb.embedding);
                (intent_emb.intent_name.clone(), sim)
            })
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut seen = std::collections::HashSet::new();
        scores.retain(|(intent, _)| seen.insert(intent.clone()));
        scores.truncate(k);
        scores
    }

    pub fn text_similarity(&mut self, text1: &str, text2: &str) -> f32 {
        let emb1 = self.embed(text1);
        let emb2 = self.embed(text2);
        Self::cosine_similarity(&emb1, &emb2)
    }

    pub fn extract_semantic_entities(&mut self, text: &str) -> HashMap<String, String> {
        let mut entities = HashMap::new();
        let text_lower = text.to_lowercase();
        let plugins = crate::plugins::get_all_plugins();
        for plugin in plugins {
            for command in &plugin.commands {
                let trigger_lower = command.trigger.to_lowercase();
                if text_lower.contains(&trigger_lower) {
                    entities.insert("plugin_trigger".to_string(), command.trigger.clone());
                    entities.insert("plugin_name".to_string(), plugin.metadata.name.clone());
                    entities.insert("plugin_action".to_string(), command.action_type.to_string());
                    break;
                }
                for keyword in command.trigger.split_whitespace() {
                    if keyword.len() > 2 && text_lower.contains(keyword) {
                        entities.insert("plugin_trigger".to_string(), command.trigger.clone());
                        entities.insert("plugin_name".to_string(), plugin.metadata.name.clone());
                        break;
                    }
                }
                for example in &command.examples {
                    if text_lower.contains(&example.to_lowercase()) {
                        entities.insert("plugin_trigger".to_string(), command.trigger.clone());
                        entities.insert("plugin_name".to_string(), plugin.metadata.name.clone());
                        break;
                    }
                }
                if entities.contains_key("plugin_trigger") {
                    break;
                }
            }
            if entities.contains_key("plugin_trigger") {
                break;
            }
        }

        let app_patterns = vec![
            ("chrome", "browser"), ("firefox", "browser"), ("edge", "browser"),
            ("safari", "browser"), ("notepad", "editor"), ("vscode", "editor"),
            ("visual studio code", "editor"), ("sublime", "editor"),
            ("calculator", "utility"), ("spotify", "music"), ("discord", "communication"),
            ("slack", "communication"), ("teams", "communication"), ("zoom", "communication"),
            ("word", "office"), ("excel", "office"), ("powerpoint", "office"),
            ("photoshop", "creative"), ("terminal", "system"), ("cmd", "system"),
            ("powershell", "system"),
        ];
        for (app_name, _) in &app_patterns {
            if text_lower.contains(app_name) {
                entities.insert("app".to_string(), app_name.to_string());
                break;
            }
        }
        if !entities.contains_key("app") {
            for word in text_lower.split_whitespace() {
                if word.len() > 2 {
                    for (app_name, _) in &app_patterns {
                        let sim = self.text_similarity(word, app_name);
                        if sim > 0.7 {
                            entities.insert("app".to_string(), app_name.to_string());
                            break;
                        }
                    }
                }
                if entities.contains_key("app") {
                    break;
                }
            }
        }

        let number_regex = regex::Regex::new(r"\b(\d+)\b").ok();
        if let Some(re) = number_regex {
            if let Some(caps) = re.captures(&text_lower) {
                if let Some(num) = caps.get(1) {
                    entities.insert("number".to_string(), num.as_str().to_string());
                }
            }
        }

        let actions = ["open", "close", "start", "stop", "launch", "quit", "exit",
                       "increase", "decrease", "set", "turn", "enable", "disable",
                       "mute", "unmute", "lock", "shutdown", "restart", "search"];
        for action in &actions {
            if text_lower.contains(action) {
                entities.insert("action".to_string(), action.to_string());
                break;
            }
        }

        let targets = [
            ("wifi", "network"), ("wi-fi", "network"), ("wireless", "network"),
            ("bluetooth", "network"), ("volume", "audio"), ("sound", "audio"),
            ("brightness", "display"), ("screen", "display"),
        ];
        for (target, category) in &targets {
            if text_lower.contains(target) {
                entities.insert("target".to_string(), target.to_string());
                entities.insert("category".to_string(), category.to_string());
                break;
            }
        }
        entities
    }

    pub fn is_loaded(&self) -> bool {
        self.model_loaded || !self.intent_embeddings.is_empty()
    }

    pub fn intent_count(&self) -> usize {
        self.intent_embeddings.len()
    }

    pub fn add_intent(&mut self, intent_name: &str, phrase: &str) {
        let embedding = if self.session.is_some() {
            self.embed_onnx(phrase).unwrap_or_else(|| self.embed_fallback(phrase))
        } else {
            self.embed_fallback(phrase)
        };
        self.intent_embeddings.push(IntentEmbedding {
            intent_name: intent_name.to_string(),
            phrase: phrase.to_string(),
            embedding,
        });
    }

    pub fn clear_cache(&mut self) {
        self.embeddings_cache.clear();
    }
}

impl Default for SbertEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SharedSbertEngine {
    engine: std::sync::Arc<std::sync::Mutex<SbertEngine>>,
}

impl SharedSbertEngine {
    pub fn new() -> Self {
        Self { engine: std::sync::Arc::new(std::sync::Mutex::new(SbertEngine::new())) }
    }

    pub fn initialize(&self) -> Result<()> {
        let mut engine = self.engine.lock().map_err(|_| anyhow!("Lock failed"))?;
        engine.initialize()
    }

    pub fn find_intent(&self, text: &str) -> Option<(String, f32)> {
        let mut engine = self.engine.lock().ok()?;
        engine.find_intent(text)
    }

    pub fn find_top_intents(&self, text: &str, k: usize) -> Vec<(String, f32)> {
        if let Ok(mut engine) = self.engine.lock() {
            engine.find_top_intents(text, k)
        } else {
            Vec::new()
        }
    }

    pub fn text_similarity(&self, text1: &str, text2: &str) -> f32 {
        if let Ok(mut engine) = self.engine.lock() {
            engine.text_similarity(text1, text2)
        } else {
            0.0
        }
    }

    pub fn extract_semantic_entities(&self, text: &str) -> HashMap<String, String> {
        if let Ok(mut engine) = self.engine.lock() {
            engine.extract_semantic_entities(text)
        } else {
            HashMap::new()
        }
    }

    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let mut engine = self.engine.lock().ok()?;
        Some(engine.embed(text))
    }

    pub fn is_loaded(&self) -> bool {
        if let Ok(engine) = self.engine.lock() {
            engine.is_loaded()
        } else {
            false
        }
    }

    pub fn add_intent(&self, intent_name: &str, phrase: &str) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.add_intent(intent_name, phrase);
        }
    }
}

impl Default for SharedSbertEngine {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    pub static ref GLOBAL_SBERT: SharedSbertEngine = SharedSbertEngine::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sbert_initialization() {
        let mut engine = SbertEngine::new();
        engine.load_precomputed_embeddings();
        assert!(engine.intent_count() > 0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((SbertEngine::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        let c = vec![0.0, 1.0, 0.0];
        assert!(SbertEngine::cosine_similarity(&a, &c).abs() < 0.001);
    }

    #[test]
    fn test_find_intent() {
        let mut engine = SbertEngine::new();
        engine.load_precomputed_embeddings();
        if let Some((intent, score)) = engine.find_intent("open chrome browser") {
            assert_eq!(intent, "open_app");
            assert!(score > 0.3);
        }
    }

    #[test]
    fn test_text_similarity() {
        let mut engine = SbertEngine::new();
        let sim1 = engine.text_similarity("open chrome", "launch chrome");
        let sim2 = engine.text_similarity("open chrome", "close firefox");
        assert!(sim1 > sim2);
    }

    #[test]
    fn test_semantic_entity_extraction() {
        let mut engine = SbertEngine::new();
        let entities = engine.extract_semantic_entities("open chrome browser");
        assert!(entities.contains_key("app"));
        assert_eq!(entities.get("app"), Some(&"chrome".to_string()));
    }
}
