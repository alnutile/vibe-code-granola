//! Chat completions against any OpenAI-compatible endpoint.
//!
//! OpenRouter, OpenAI, Ollama, LM Studio, vLLM, llama.cpp's server and a dozen
//! others all speak the same `/chat/completions` dialect. That is the whole
//! reason this file is short: switching from a frontier model to something
//! running on localhost is a base URL and a model string, not a new code path.

use crate::error::{AppError, Result};
use crate::settings::{SecretStore, Settings};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

impl ChatMsg {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMsg],
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Option<ResponseMessage>,
    delta: Option<ResponseMessage>,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Free-form, provider-specific. Shown as a hint in the model picker.
    pub description: Option<String>,
    pub context_length: Option<u64>,
}

pub struct LlmClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    temperature: f32,
    max_tokens: Option<u32>,
    provider: String,
}

impl LlmClient {
    pub fn from_settings(settings: &Settings, secrets: &SecretStore) -> Result<Self> {
        let provider = settings.llm.provider.clone();

        let api_key = match settings.llm_secret_key() {
            Some(key) => Some(secrets.require(key, &provider)?),
            None => None,
        };

        if settings.llm.model.trim().is_empty() {
            return Err(AppError::Config(
                "No chat model selected. Pick one in Settings → Models.".into(),
            ));
        }

        Ok(Self {
            http: reqwest::Client::builder()
                // Long enough for a slow local model to finish a write-up; short
                // enough that a wedged endpoint surfaces rather than hangs forever.
                .timeout(Duration::from_secs(300))
                .build()?,
            base_url: normalize_base_url(&settings.llm.base_url),
            api_key,
            model: settings.llm.model.clone(),
            temperature: settings.llm.temperature,
            max_tokens: settings.llm.max_tokens,
            provider,
        })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.http.request(method, format!("{}{}", self.base_url, path));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        if self.provider == "openrouter" {
            // OpenRouter attributes usage to these; harmless everywhere else, but
            // only sent where it means something.
            req = req
                .header("HTTP-Referer", "https://github.com/alnutile/vibecode-granola")
                .header("X-Title", "vibecode-granola");
        }
        req
    }

    /// One-shot completion. Used for suggestions, note generation, and titles.
    pub async fn complete(&self, messages: &[ChatMsg]) -> Result<String> {
        let body = ChatRequest {
            model: &self.model,
            messages,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stream: false,
        };

        let res = self
            .request(reqwest::Method::POST, "/chat/completions")
            .json(&body)
            .send()
            .await?;

        let res = self.check_status(res).await?;
        let parsed: ChatResponse = res.json().await?;

        Ok(parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.or(c.delta))
            .and_then(|m| m.content)
            .unwrap_or_default()
            .trim()
            .to_string())
    }

    /// Streaming completion. `on_delta` fires for each token chunk; the full
    /// concatenated text is returned once the stream ends.
    ///
    /// Used for meeting chat, where waiting for a complete answer feels broken.
    pub async fn stream<F>(&self, messages: &[ChatMsg], mut on_delta: F) -> Result<String>
    where
        F: FnMut(&str),
    {
        let body = ChatRequest {
            model: &self.model,
            messages,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stream: true,
        };

        let res = self
            .request(reqwest::Method::POST, "/chat/completions")
            .json(&body)
            .send()
            .await?;

        let res = self.check_status(res).await?;

        let mut full = String::new();
        let mut buffer = String::new();
        let mut stream = res.bytes_stream();

        while let Some(chunk) = stream.next().await {
            buffer.push_str(&String::from_utf8_lossy(&chunk?));

            // SSE frames are newline-delimited, but a single TCP chunk can split
            // one mid-line — so only consume up to the last complete newline and
            // leave the remainder in the buffer.
            while let Some(newline) = buffer.find('\n') {
                let line = buffer[..newline].trim().to_string();
                buffer.drain(..=newline);

                let Some(data) = line.strip_prefix("data:") else {
                    continue; // comments, `event:` lines, blank separators
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }

                // A malformed frame shouldn't kill an otherwise good stream.
                let Ok(parsed) = serde_json::from_str::<ChatResponse>(data) else {
                    continue;
                };
                if let Some(text) = parsed
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|c| c.delta.or(c.message))
                    .and_then(|m| m.content)
                {
                    if !text.is_empty() {
                        on_delta(&text);
                        full.push_str(&text);
                    }
                }
            }
        }

        Ok(full.trim().to_string())
    }

    /// Model list for the picker. Providers vary in what they return, so this
    /// handles both the OpenAI `{data: [{id}]}` shape and OpenRouter's richer one.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<RawModel>,
        }
        #[derive(Deserialize)]
        struct RawModel {
            id: String,
            name: Option<String>,
            description: Option<String>,
            context_length: Option<u64>,
        }

        let res = self.request(reqwest::Method::GET, "/models").send().await?;
        let res = self.check_status(res).await?;
        let parsed: ModelsResponse = res.json().await?;

        let mut models: Vec<ModelInfo> = parsed
            .data
            .into_iter()
            .map(|m| ModelInfo {
                name: m.name.unwrap_or_else(|| m.id.clone()),
                id: m.id,
                description: m.description,
                context_length: m.context_length,
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    /// Cheap round-trip to prove the endpoint, key, and model all work together.
    pub async fn test_connection(&self) -> Result<String> {
        let reply = self
            .complete(&[ChatMsg::user("Reply with the single word: ok")])
            .await?;
        Ok(format!("{} · {} → {}", self.provider, self.model, reply))
    }

    /// Turn a non-2xx response into an error carrying the provider's own message.
    /// The body is where the useful part lives ("insufficient credits", "no such
    /// model"), so it goes in the error rather than being discarded.
    async fn check_status(&self, res: reqwest::Response) -> Result<reqwest::Response> {
        if res.status().is_success() {
            return Ok(res);
        }
        let status = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        Err(AppError::Provider {
            provider: self.provider.clone(),
            status,
            body: truncate(&body, 500),
        })
    }
}

/// Accept base URLs with or without a trailing slash. Users paste both.
pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

/// Sensible base URL for a provider, used to prefill the field when the user
/// switches providers in Settings.
pub fn default_base_url(provider: &str) -> &'static str {
    match provider {
        "openrouter" => "https://openrouter.ai/api/v1",
        "openai" => "https://api.openai.com/v1",
        "ollama" => "http://localhost:11434/v1",
        "lmstudio" => "http://localhost:1234/v1",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls_normalize_regardless_of_trailing_slash() {
        assert_eq!(
            normalize_base_url("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(
            normalize_base_url("  http://localhost:11434/v1  "),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        assert_eq!(truncate("short", 100), "short");
        assert_eq!(truncate("abcdef", 3), "abc…");
    }
}
