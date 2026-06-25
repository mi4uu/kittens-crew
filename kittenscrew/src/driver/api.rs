//! T60 — the Driver seam: a backend-agnostic boundary between the deterministic
//! DAG/verify core and whatever model runs a turn. The loop reaches the model
//! ONLY through this trait (ironclaw's host-port), so the invariant-checking core
//! stays pure and testable. First backend: `HttpDriver` speaking OpenAI-format
//! `/chat/completions` — Codestral by default (free, coding-specialised; gemma is
//! weak at code, so the coding role routes here). `ApiDriver` via rig (T61) and
//! `ClaudeCodeDriver` via tmux (T71) are later backends behind this same trait.

use std::time::Duration;

/// One scoped instruction for a node. Kept minimal for the thinnest proof — the
/// struct has room to grow (tools, role, model-pick) without touching callers.
pub struct Turn {
    pub prompt: String,
}

/// What one model turn produced.
pub struct TurnResult {
    pub text: String,
    /// The model the backend actually answered with (failover/routing may differ
    /// from what we asked — record what really ran).
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("missing env var {0}")]
    MissingKey(String),
    #[error("http: {0}")]
    Http(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("model returned no text content")]
    Empty,
}

/// The seam. The drive loop (T62) reaches the model only through this.
pub trait Driver {
    fn dispatch(&self, turn: &Turn) -> Result<TurnResult, DriverError>;
    fn model(&self) -> &str;
}

/// OpenAI-format chat backend (one provider, one pinned model — no failover here;
/// routing/failover is a separate concern, the future routing server).
pub struct HttpDriver {
    endpoint: String,
    api_key: String,
    model: String,
    max_tokens: u32,
    timeout: Duration,
}

impl HttpDriver {
    /// Codestral free coding model. Key from `CODESTRAL_API_KEY` (never hardcoded —
    /// fail fast if absent, per security rules).
    pub fn codestral() -> Result<Self, DriverError> {
        let api_key = std::env::var("CODESTRAL_API_KEY")
            .map_err(|_| DriverError::MissingKey("CODESTRAL_API_KEY".into()))?;
        Ok(HttpDriver {
            endpoint: "https://codestral.mistral.ai/v1/chat/completions".into(),
            api_key,
            model: "codestral-latest".into(),
            max_tokens: 1024,
            timeout: Duration::from_secs(90),
        })
    }

    /// Generic OpenAI-compatible `/chat/completions` endpoint — codestral,
    /// openrouter direct, or a LOCAL server (LM Studio :1234, ollama :11434).
    /// `base_url` is the API root WITHOUT `/chat/completions`
    /// (e.g. `http://localhost:1234/v1`); `api_key` may be a dummy for local servers.
    /// Higher `max_tokens` + timeout than codestral so a slow local reasoning model
    /// (which spends tokens thinking before it answers) still emits a full file.
    pub fn openai(base_url: &str, model: impl Into<String>, api_key: impl Into<String>) -> Self {
        HttpDriver {
            endpoint: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            api_key: api_key.into(),
            model: model.into(),
            max_tokens: 4096,
            timeout: Duration::from_secs(180),
        }
    }
}

impl Driver for HttpDriver {
    fn dispatch(&self, turn: &Turn) -> Result<TurnResult, DriverError> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": [{ "role": "user", "content": turn.prompt }],
        });
        let resp = ureq::post(&self.endpoint)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .timeout(self.timeout)
            .send_json(body)
            .map_err(|e| DriverError::Http(e.to_string()))?;
        let v: serde_json::Value = resp
            .into_json()
            .map_err(|e| DriverError::Http(format!("decode: {e}")))?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(DriverError::Empty)?
            .to_string();
        let model = v["model"].as_str().unwrap_or(&self.model).to_string();
        Ok(TurnResult { text, model })
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// T61 — multi-provider backend built on the `rig` crate (rig-core), which speaks
/// many LLM APIs behind one client. `rig` is async; our `Driver` seam is sync and
/// must STAY sync (changing it would ripple through drive.rs/delegation.rs/
/// scenario.rs and every caller). So the async is fully contained here: the struct
/// owns a tokio runtime and `block_on`s the rig call inside the sync `dispatch`.
///
/// Targets any OpenAI-compatible `/chat/completions` endpoint (codestral,
/// openrouter, groq, local llamafile, …) via rig's OpenAI Completions client with a
/// configurable `base_url`. Provider/model/key come from `[driver]` config or the
/// constructor; the key is read from an env var (never hardcoded, per security
/// rules — fail fast if absent).
pub struct RigDriver {
    runtime: tokio::runtime::Runtime,
    client: rig_core::providers::openai::CompletionsClient,
    model: String,
}

impl RigDriver {
    /// Build a driver against an OpenAI-compatible endpoint.
    /// - `base_url`: provider endpoint up to (not including) `/chat/completions`,
    ///   e.g. `https://codestral.mistral.ai/v1`. `None` → rig's default (OpenAI).
    /// - `model`: the model id to ask for.
    /// - `api_key`: the bearer key (already resolved from env by the caller).
    pub fn new(
        base_url: Option<&str>,
        model: impl Into<String>,
        api_key: &str,
    ) -> Result<Self, DriverError> {
        let mut builder =
            rig_core::providers::openai::CompletionsClient::builder().api_key(api_key);
        if let Some(base) = base_url {
            builder = builder.base_url(base);
        }
        let client = builder
            .build()
            .map_err(|e| DriverError::Provider(format!("client build: {e}")))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| DriverError::Provider(format!("tokio runtime: {e}")))?;
        Ok(RigDriver {
            runtime,
            client,
            model: model.into(),
        })
    }

    /// Construct from `provider`/`model` + the key env var, the way
    /// `HttpDriver::codestral()` reads `CODESTRAL_API_KEY`. `base_url` and the
    /// env var name are threaded through so any OpenAI-compatible provider works.
    pub fn from_env(
        base_url: Option<&str>,
        model: impl Into<String>,
        key_env: &str,
    ) -> Result<Self, DriverError> {
        let api_key =
            std::env::var(key_env).map_err(|_| DriverError::MissingKey(key_env.into()))?;
        Self::new(base_url, model, &api_key)
    }

    /// Codestral via rig, for parity with `HttpDriver::codestral()` (same model,
    /// same key env, but through the multi-provider rig client).
    pub fn codestral() -> Result<Self, DriverError> {
        Self::from_env(
            Some("https://codestral.mistral.ai/v1"),
            "codestral-latest",
            "CODESTRAL_API_KEY",
        )
    }
}

impl Driver for RigDriver {
    fn dispatch(&self, turn: &Turn) -> Result<TurnResult, DriverError> {
        use rig_core::client::CompletionClient;
        use rig_core::completion::Prompt;

        let agent = self.client.agent(&self.model).build();
        let prompt = turn.prompt.clone();
        let text = self
            .runtime
            .block_on(async move { agent.prompt(prompt).await })
            .map_err(|e| DriverError::Provider(e.to_string()))?;
        if text.trim().is_empty() {
            return Err(DriverError::Empty);
        }
        Ok(TurnResult {
            text,
            model: self.model.clone(),
        })
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic in-memory Driver — lets the loop be tested with no network.
    pub struct EchoDriver(pub String);
    impl Driver for EchoDriver {
        fn dispatch(&self, _t: &Turn) -> Result<TurnResult, DriverError> {
            Ok(TurnResult {
                text: self.0.clone(),
                model: "echo".into(),
            })
        }
        fn model(&self) -> &str {
            "echo"
        }
    }

    #[test]
    fn echo_driver_returns_canned_text() {
        let d = EchoDriver("```rust\nfn x() {}\n```".into());
        let r = d.dispatch(&Turn { prompt: "x".into() }).unwrap();
        assert!(r.text.contains("fn x()"));
        assert_eq!(r.model, "echo");
    }

    #[test]
    fn codestral_without_key_is_missing_key_err() {
        // Don't assume the env is unset in CI; only assert the error shape when it is.
        if std::env::var("CODESTRAL_API_KEY").is_err() {
            assert!(matches!(
                HttpDriver::codestral(),
                Err(DriverError::MissingKey(_))
            ));
        }
    }

    /// Real network smoke test against Codestral. Run manually:
    ///   CODESTRAL_API_KEY=... cargo test -- --ignored codestral_smoke
    #[test]
    #[ignore]
    fn codestral_smoke_returns_rust() {
        let d = HttpDriver::codestral().expect("CODESTRAL_API_KEY set");
        let r = d
            .dispatch(&Turn {
                prompt: "Write a Rust fn add(a:i64,b:i64)->i64. Only a fenced code block.".into(),
            })
            .expect("dispatch");
        assert!(r.text.contains("fn add"), "got: {}", r.text);
    }

    #[test]
    fn rig_codestral_without_key_is_missing_key_err() {
        if std::env::var("CODESTRAL_API_KEY").is_err() {
            assert!(matches!(
                RigDriver::codestral(),
                Err(DriverError::MissingKey(_))
            ));
        }
    }

    /// Real network smoke test against a provider through rig. Run manually:
    ///   CODESTRAL_API_KEY=... cargo test -- --ignored rig_codestral_smoke
    #[test]
    #[ignore]
    fn rig_codestral_smoke_returns_rust() {
        let d = RigDriver::codestral().expect("CODESTRAL_API_KEY set");
        let r = d
            .dispatch(&Turn {
                prompt: "Write a Rust fn add(a:i64,b:i64)->i64. Only a fenced code block.".into(),
            })
            .expect("dispatch");
        assert!(r.text.contains("fn add"), "got: {}", r.text);
    }
}
