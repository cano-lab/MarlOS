//! F5-TTS voice cloning sidecar manager.
//!
//! Spawns `python/tts_sidecar.py` lazily on first use and speaks a
//! newline-delimited JSON-RPC protocol over its stdin/stdout. Designed for
//! single-flight requests (the model is sequential).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum VoiceTtsError {
    #[error("python interpreter not found — set MARLOS_PYTHON or create python/.venv")]
    PythonNotFound,
    #[error("sidecar IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sidecar protocol error: {0}")]
    Protocol(String),
    #[error("sidecar reported error: {0}")]
    Sidecar(String),
    #[error("sidecar exited unexpectedly")]
    Exited,
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VoiceTtsStatus {
    pub running: bool,
    pub model_loaded: bool,
    pub has_reference: bool,
    pub reference_audio_path: Option<String>,
    pub reference_transcript: Option<String>,
    pub python_path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SynthesizeResult {
    /// WAV bytes, base64-encoded.
    pub wav_b64: String,
    pub sample_rate: u32,
    pub samples: u64,
}

struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Sidecar {
    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, VoiceTtsError> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let req = json!({ "id": id, "method": method, "params": params });
        let line = serde_json::to_string(&req)?;

        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        let mut buf = String::new();
        loop {
            buf.clear();
            let n = self.stdout.read_line(&mut buf).await?;
            if n == 0 {
                return Err(VoiceTtsError::Exited);
            }
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            let resp: Value = serde_json::from_str(trimmed).map_err(|e| {
                VoiceTtsError::Protocol(format!("invalid JSON line `{}`: {}", trimmed, e))
            })?;

            // Skip async events that have null id (e.g. the initial "ready")
            if resp.get("id").map(|v| v.is_null()).unwrap_or(true) {
                continue;
            }
            if resp.get("id").and_then(|v| v.as_str()) != Some(id.as_str()) {
                // Mismatched id — protocol violation; treat as error
                return Err(VoiceTtsError::Protocol(format!(
                    "id mismatch: expected {}, got {:?}",
                    id,
                    resp.get("id")
                )));
            }

            let ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            if !ok {
                let msg = resp
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown sidecar error")
                    .to_string();
                return Err(VoiceTtsError::Sidecar(msg));
            }
            return Ok(resp.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

pub struct VoiceTtsManager {
    sidecar: Mutex<Option<Sidecar>>,
    reference_audio: Mutex<Option<PathBuf>>,
    reference_transcript: Mutex<Option<String>>,
    python_path: Mutex<Option<PathBuf>>,
    /// Where reference audio is persisted on disk.
    audio_storage_path: PathBuf,
    /// Where the python script lives.
    script_path: PathBuf,
}

impl VoiceTtsManager {
    pub fn new(app_data_dir: PathBuf, script_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            sidecar: Mutex::new(None),
            reference_audio: Mutex::new(None),
            reference_transcript: Mutex::new(None),
            python_path: Mutex::new(None),
            audio_storage_path: app_data_dir.join("voice_reference.wav"),
            script_path,
        })
    }

    /// Resolve a python interpreter path, preferring MARLOS_PYTHON, then a
    /// project-local venv, then `python` on PATH.
    fn resolve_python() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MARLOS_PYTHON") {
            let path = PathBuf::from(p);
            if path.exists() {
                return Some(path);
            }
        }
        // Project-local venv (matches script_path.parent())
        // Caller can override via env var; fall through to system python otherwise.
        let venv_candidates = if cfg!(windows) {
            vec!["python\\.venv\\Scripts\\python.exe"]
        } else {
            vec!["python/.venv/bin/python", "python/.venv/bin/python3"]
        };
        for rel in venv_candidates {
            let p = PathBuf::from(rel);
            if p.exists() {
                return Some(p);
            }
        }
        // Fall back to system python
        if which_python("python").is_some() {
            return Some(PathBuf::from("python"));
        }
        if which_python("python3").is_some() {
            return Some(PathBuf::from("python3"));
        }
        None
    }

    async fn ensure_started(&self) -> Result<(), VoiceTtsError> {
        let mut guard = self.sidecar.lock().await;
        if guard.is_some() {
            return Ok(());
        }

        let python = Self::resolve_python().ok_or(VoiceTtsError::PythonNotFound)?;
        log::info!(
            "Starting voice TTS sidecar: {} {}",
            python.display(),
            self.script_path.display()
        );

        let mut cmd = Command::new(&python);
        cmd.arg(&self.script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        // Avoid Python output buffering on the response stream
        cmd.env("PYTHONUNBUFFERED", "1");

        let mut child = cmd.spawn().map_err(VoiceTtsError::Io)?;
        let stdin = child.stdin.take().ok_or_else(|| {
            VoiceTtsError::Protocol("failed to capture sidecar stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            VoiceTtsError::Protocol("failed to capture sidecar stdout".to_string())
        })?;

        *self.python_path.lock().await = Some(python);

        *guard = Some(Sidecar {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        });

        Ok(())
    }

    pub async fn ping(&self) -> Result<Value, VoiceTtsError> {
        self.ensure_started().await?;
        let mut guard = self.sidecar.lock().await;
        let sc = guard.as_mut().ok_or(VoiceTtsError::Exited)?;
        sc.send_request("ping", json!({})).await
    }

    pub async fn status(&self) -> VoiceTtsStatus {
        let running = self.sidecar.lock().await.is_some();
        let python_path = self
            .python_path
            .lock()
            .await
            .as_ref()
            .map(|p| p.display().to_string());

        let mut model_loaded = false;
        if running {
            if let Ok(v) = self.ping().await {
                model_loaded = v
                    .get("model_loaded")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
            }
        }

        let reference_audio_path = self
            .reference_audio
            .lock()
            .await
            .as_ref()
            .map(|p| p.display().to_string());
        let reference_transcript = self.reference_transcript.lock().await.clone();
        let has_reference = reference_audio_path.is_some() && reference_transcript.is_some();

        VoiceTtsStatus {
            running,
            model_loaded,
            has_reference,
            reference_audio_path,
            reference_transcript,
            python_path,
        }
    }

    pub async fn set_reference(
        &self,
        wav_bytes: &[u8],
        transcript: String,
    ) -> Result<(), VoiceTtsError> {
        if let Some(parent) = self.audio_storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.audio_storage_path, wav_bytes)?;

        self.ensure_started().await?;
        let path = self.audio_storage_path.clone();
        {
            let mut guard = self.sidecar.lock().await;
            let sc = guard.as_mut().ok_or(VoiceTtsError::Exited)?;
            sc.send_request(
                "set_reference",
                json!({
                    "audio_path": path.to_string_lossy(),
                    "transcript": transcript,
                }),
            )
            .await?;
        }

        *self.reference_audio.lock().await = Some(path);
        *self.reference_transcript.lock().await = Some(transcript);
        Ok(())
    }

    pub async fn clear_reference(&self) -> Result<(), VoiceTtsError> {
        // Best-effort sidecar notify (only if running)
        let running = self.sidecar.lock().await.is_some();
        if running {
            let mut guard = self.sidecar.lock().await;
            if let Some(sc) = guard.as_mut() {
                let _ = sc.send_request("clear_reference", json!({})).await;
            }
        }

        if self.audio_storage_path.exists() {
            let _ = std::fs::remove_file(&self.audio_storage_path);
        }
        *self.reference_audio.lock().await = None;
        *self.reference_transcript.lock().await = None;
        Ok(())
    }

    pub async fn synthesize(
        &self,
        text: String,
        speed: Option<f32>,
        nfe: Option<u32>,
    ) -> Result<SynthesizeResult, VoiceTtsError> {
        if self.reference_audio.lock().await.is_none() {
            return Err(VoiceTtsError::Sidecar(
                "no reference voice set — record a reference first".to_string(),
            ));
        }

        self.ensure_started().await?;
        // If sidecar was started before reference was set (or reset), re-send the reference
        self.refresh_reference_if_needed().await?;

        let mut guard = self.sidecar.lock().await;
        let sc = guard.as_mut().ok_or(VoiceTtsError::Exited)?;
        let mut params = json!({ "text": text });
        if let Some(s) = speed {
            params["speed"] = json!(s);
        }
        if let Some(n) = nfe {
            params["nfe"] = json!(n);
        }
        let v = sc.send_request("synthesize", params).await?;
        let result: SynthesizeResult = serde_json::from_value(v)?;
        Ok(result)
    }

    async fn refresh_reference_if_needed(&self) -> Result<(), VoiceTtsError> {
        let path = self.reference_audio.lock().await.clone();
        let transcript = self.reference_transcript.lock().await.clone();
        let (Some(path), Some(transcript)) = (path, transcript) else {
            return Ok(());
        };
        // Cheap re-set; sidecar handler is idempotent
        let mut guard = self.sidecar.lock().await;
        let sc = guard.as_mut().ok_or(VoiceTtsError::Exited)?;
        let v = sc.send_request("ping", json!({})).await?;
        let has_ref = v.get("has_reference").and_then(|b| b.as_bool()).unwrap_or(false);
        if has_ref {
            return Ok(());
        }
        sc.send_request(
            "set_reference",
            json!({
                "audio_path": path.to_string_lossy(),
                "transcript": transcript,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        let mut guard = self.sidecar.lock().await;
        if let Some(mut sc) = guard.take() {
            let _ = sc.send_request("shutdown", json!({})).await;
            let _ = sc.child.start_kill();
        }
    }
}

fn which_python(name: &str) -> Option<PathBuf> {
    // Cheap PATH lookup using std::env::split_paths
    let path = std::env::var_os("PATH")?;
    let exe = if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    for dir in std::env::split_paths(&path) {
        let candidate: PathBuf = Path::new(&dir).join(&exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
