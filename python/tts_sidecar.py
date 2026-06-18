#!/usr/bin/env python3
"""
MarlOS F5-TTS Voice Cloning Sidecar.

A long-lived process spawned by the Tauri Rust backend. Speaks JSON-RPC over
stdin/stdout, one request per line. Loads the F5-TTS model lazily on the
first synthesis request to keep startup fast.

Protocol (newline-delimited JSON):
  -> {"id": "1", "method": "ping"}
  <- {"id": "1", "ok": true, "result": {"alive": true}}

  -> {"id": "2", "method": "set_reference",
       "params": {"audio_path": "...", "transcript": "..."}}
  <- {"id": "2", "ok": true, "result": {"set": true}}

  -> {"id": "3", "method": "synthesize", "params": {"text": "Hello"}}
  <- {"id": "3", "ok": true, "result": {"wav_b64": "...", "sample_rate": 24000}}

  -> {"id": "4", "method": "shutdown"}
  <- {"id": "4", "ok": true, "result": {"bye": true}}

All log lines go to stderr to keep stdout pure JSON.
"""

import base64
import io
import json
import os
import sys
import traceback
from typing import Any, Optional

import numpy as np
import soundfile as sf

# Lazy-loaded module-level state
_F5 = None
_REFERENCE_AUDIO: Optional[str] = None
_REFERENCE_TEXT: Optional[str] = None


def log(msg: str) -> None:
    print(f"[tts_sidecar] {msg}", file=sys.stderr, flush=True)


def write_response(resp: dict) -> None:
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()


def ensure_model():
    """Lazy-load F5-TTS on first synthesis call."""
    global _F5
    if _F5 is not None:
        return _F5

    log("Loading F5-TTS (first call) — this may take ~30s...")
    # Import lazily so a missing torch install doesn't kill the sidecar at boot
    from f5_tts.api import F5TTS  # type: ignore

    # Defaults to F5TTS_v1_Base, downloads weights on first run to HF cache
    _F5 = F5TTS()
    log("F5-TTS loaded")
    return _F5


def handle_ping(_params: dict) -> dict:
    has_ref = _REFERENCE_AUDIO is not None and _REFERENCE_TEXT is not None
    return {"alive": True, "model_loaded": _F5 is not None, "has_reference": has_ref}


def handle_set_reference(params: dict) -> dict:
    global _REFERENCE_AUDIO, _REFERENCE_TEXT
    audio_path = params.get("audio_path")
    transcript = params.get("transcript")
    if not audio_path or not transcript:
        raise ValueError("set_reference requires audio_path and transcript")
    if not os.path.isfile(audio_path):
        raise FileNotFoundError(f"reference audio not found: {audio_path}")

    _REFERENCE_AUDIO = audio_path
    _REFERENCE_TEXT = transcript.strip()
    log(f"Reference set: {audio_path} ({len(_REFERENCE_TEXT)} chars transcript)")
    return {"set": True}


def handle_clear_reference(_params: dict) -> dict:
    global _REFERENCE_AUDIO, _REFERENCE_TEXT
    _REFERENCE_AUDIO = None
    _REFERENCE_TEXT = None
    return {"cleared": True}


def handle_synthesize(params: dict) -> dict:
    text = (params.get("text") or "").strip()
    if not text:
        raise ValueError("synthesize requires non-empty text")
    if _REFERENCE_AUDIO is None or _REFERENCE_TEXT is None:
        raise RuntimeError("no reference voice set — call set_reference first")

    speed = float(params.get("speed", 1.0))
    nfe = int(params.get("nfe", 32))  # number of function evaluations (quality vs speed)

    model = ensure_model()
    log(f"Synthesizing {len(text)} chars (speed={speed}, nfe={nfe})...")

    wav, sr, _ = model.infer(
        ref_file=_REFERENCE_AUDIO,
        ref_text=_REFERENCE_TEXT,
        gen_text=text,
        speed=speed,
        nfe_step=nfe,
        remove_silence=True,
    )

    # wav is a numpy array float32 in [-1, 1]
    if isinstance(wav, np.ndarray):
        audio = wav.astype(np.float32)
    else:
        audio = np.asarray(wav, dtype=np.float32)

    buf = io.BytesIO()
    sf.write(buf, audio, sr, format="WAV", subtype="PCM_16")
    wav_bytes = buf.getvalue()
    return {
        "wav_b64": base64.b64encode(wav_bytes).decode("ascii"),
        "sample_rate": int(sr),
        "samples": int(audio.shape[-1]),
    }


HANDLERS = {
    "ping": handle_ping,
    "set_reference": handle_set_reference,
    "clear_reference": handle_clear_reference,
    "synthesize": handle_synthesize,
}


def main() -> int:
    log(f"Sidecar starting (python {sys.version.split()[0]})")
    write_response({"id": None, "ok": True, "result": {"event": "ready"}})

    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        req_id: Any = None
        try:
            req = json.loads(raw)
            req_id = req.get("id")
            method = req.get("method")
            params = req.get("params") or {}

            if method == "shutdown":
                write_response({"id": req_id, "ok": True, "result": {"bye": True}})
                log("Shutdown requested")
                return 0

            handler = HANDLERS.get(method)
            if handler is None:
                raise ValueError(f"unknown method: {method}")

            result = handler(params)
            write_response({"id": req_id, "ok": True, "result": result})
        except Exception as e:
            log(f"Error handling request: {e}\n{traceback.format_exc()}")
            write_response({
                "id": req_id,
                "ok": False,
                "error": str(e),
                "type": type(e).__name__,
            })
    log("stdin closed, exiting")
    return 0


if __name__ == "__main__":
    sys.exit(main())
