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
}
