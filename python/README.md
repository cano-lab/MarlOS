# MarlOS Voice Cloning Sidecar (F5-TTS)

`tts_sidecar.py` runs as a long-lived child process spawned by the Tauri
backend. It loads F5-TTS once and exposes a JSON-line protocol on stdio for
zero-shot voice cloning from a reference audio + transcript.

## One-time setup

You need Python 3.10+ on PATH. Recommended: a dedicated venv.

```sh
cd python
python -m venv .venv
# Windows
.venv\Scripts\activate
# macOS/Linux
source .venv/bin/activate

pip install -r requirements.txt
```

The first synthesis call downloads the F5-TTS weights (~1.5 GB) into your
HuggingFace cache (`~/.cache/huggingface/`). After that, startup is fast.

If you have an NVIDIA GPU, install a CUDA-enabled torch build first:

```sh
pip install torch torchaudio --index-url https://download.pytorch.org/whl/cu121
pip install -r requirements.txt
```

## Configuring MarlOS to find Python

The Rust sidecar manager looks for the Python interpreter in this order:
1. `MARLOS_PYTHON` env var (set this to the venv python — recommended)
2. `python/.venv/Scripts/python.exe` (Windows) or `python/.venv/bin/python` (POSIX)
3. `python` on PATH

Set the env var to point at your venv for reliability:

```sh
# Windows (PowerShell)
$env:MARLOS_PYTHON = "F:\CanoLab\MarlOS\python\.venv\Scripts\python.exe"
# POSIX
export MARLOS_PYTHON="$PWD/python/.venv/bin/python"
```

## Manual smoke test

```sh
python tts_sidecar.py
# Then paste a JSON line:
{"id":"1","method":"ping"}
```
