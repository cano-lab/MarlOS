# Accessibility TODO

Accessibility features for Marlos. Everything here must be **pure Rust** so it
also runs on SemOS (no Python sidecars, no non-Rust runtimes — see the
"Marlos = pure Rust" rule). Native-host Python sidecars are a stopgap only and
must be ported before a feature is considered done.

## Open

### Text-to-speech / voice cloning — port to Rust (priority)
- **Scope:** opt-in, on-demand feature. The Python process is lazily spawned by
  `VoiceTtsManager::ensure_started` (`src-tauri/src/voice_tts.rs`) only when a
  user clones a voice or synthesizes speech — nothing starts at app launch. If
  voice cloning is never used, Python is never invoked and the rest of the app
  runs fine without it (including on SemOS). The dependency is scoped to this
  one feature, not the whole app.
- **Current state:** implemented as a Python sidecar (`python/tts_sidecar.py`,
  F5-TTS over a JSON-line stdio protocol spawned by the Tauri backend).
  Rust/UI side: `voice-clone-service.ts`, `src-tauri/src/waveform_similarity.rs`,
  `VoiceCloneSettings.tsx`.
- **Problem:** the Python sidecar can't run on SemOS and violates the pure-Rust
  rule. It needs Python 3.10+, a venv, torch, and a ~1.5 GB HF download.
- **Goal:** zero-shot voice cloning + TTS in pure Rust so it runs on SemOS.
- **Candidate paths:** ONNX Runtime (`ort`), `candle`, or `burn` to run the
  TTS model weights natively; reuse `waveform_similarity.rs` for reference-audio
  matching.
- **Done when:** the `python/` sidecar can be deleted and TTS runs in-process.

## Backlog
- Screen-reader / ARIA review of the UI.
- Keyboard-only navigation pass.
- High-contrast / dyslexia-friendly reading modes.
