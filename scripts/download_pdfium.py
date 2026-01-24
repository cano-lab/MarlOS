#!/usr/bin/env python3
"""Download PDFium library for the current platform."""

import os
import sys
import platform
import urllib.request
import zipfile
import tarfile
import shutil
from pathlib import Path

# PDFium builds from bblanchon (community builds)
PDFIUM_VERSION = "7643"
BASE_URL = f"https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/{PDFIUM_VERSION}"

def get_platform_info():
    """Get platform-specific download info."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "windows":
        if machine in ("amd64", "x86_64"):
            return "win-x64", "pdfium.dll", ".tgz"
        elif machine in ("x86", "i386", "i686"):
            return "win-x86", "pdfium.dll", ".tgz"
        elif machine in ("arm64", "aarch64"):
            return "win-arm64", "pdfium.dll", ".tgz"
    elif system == "darwin":
        if machine in ("arm64", "aarch64"):
            return "mac-arm64", "libpdfium.dylib", ".tgz"
        else:
            return "mac-x64", "libpdfium.dylib", ".tgz"
    elif system == "linux":
        if machine in ("x86_64", "amd64"):
            return "linux-x64", "libpdfium.so", ".tgz"
        elif machine in ("arm64", "aarch64"):
            return "linux-arm64", "libpdfium.so", ".tgz"
        elif machine in ("armv7l", "arm"):
            return "linux-arm", "libpdfium.so", ".tgz"

    raise RuntimeError(f"Unsupported platform: {system} {machine}")

def download_pdfium(target_dir: Path):
    """Download and extract PDFium library."""
    platform_name, lib_name, ext = get_platform_info()

    # Create target directory
    target_dir.mkdir(parents=True, exist_ok=True)

    # Download URL
    filename = f"pdfium-{platform_name}{ext}"
    url = f"{BASE_URL}/{filename}"

    print(f"Downloading PDFium for {platform_name}...")
    print(f"URL: {url}")

    # Download
    download_path = target_dir / filename
    try:
        urllib.request.urlretrieve(url, download_path)
    except urllib.error.HTTPError as e:
        # Try alternative source
        alt_url = f"https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F6666/pdfium-{platform_name}{ext}"
        print(f"Primary download failed, trying alternative...")
        urllib.request.urlretrieve(alt_url, download_path)

    print(f"Downloaded: {download_path}")

    # Extract
    print("Extracting...")
    if ext == ".zip":
        with zipfile.ZipFile(download_path, 'r') as zf:
            zf.extractall(target_dir)
    else:
        with tarfile.open(download_path, 'r:gz') as tf:
            tf.extractall(target_dir)

    # Find and copy library to expected location
    lib_path = None
    for root, dirs, files in os.walk(target_dir):
        for f in files:
            if f == lib_name or f.startswith("pdfium"):
                lib_path = Path(root) / f
                break
        if lib_path:
            break

    if lib_path and lib_path.exists():
        # Copy to target directory root
        dest = target_dir / lib_name
        if lib_path != dest:
            shutil.copy2(lib_path, dest)
        print(f"Library ready: {dest}")

        # Also copy to src-tauri directory
        tauri_lib = target_dir.parent / "src-tauri" / lib_name
        shutil.copy2(lib_path, tauri_lib)
        print(f"Copied to: {tauri_lib}")
    else:
        print(f"Warning: Could not find {lib_name} in extracted files")

    # Cleanup download
    download_path.unlink()
    print("Done!")

if __name__ == "__main__":
    # Get project root
    script_dir = Path(__file__).parent
    project_dir = script_dir.parent
    lib_dir = project_dir / "lib"

    download_pdfium(lib_dir)
