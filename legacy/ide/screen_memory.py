"""
Visual Memory System - Screen Capture and Summarization

Captures periodic screenshots of the screen and summarizes them using AI/vision.
Creates an episodic visual memory of your work that can be queried later.
"""

import os
import time
import json
import hashlib
import threading
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import List, Dict, Optional, Callable, Any
from collections import deque

import numpy as np

try:
    from PIL import Image, ImageGrab
    import cv2
    HAS_PIL = True
    HAS_CV2 = True
except ImportError:
    HAS_PIL = False
    HAS_CV2 = False
    # Fallback implementations will be provided

from kernel import SemanticKernel, MemoryType


@dataclass
class ScreenCapture:
    """A single screen capture with metadata."""
    timestamp: float
    image_path: str
    thumbnail_path: str
    summary: str = ""
    ocr_text: str = ""
    active_window: str = ""
    app_name: str = ""
    hash: str = ""
    screen_count: int = 0  # Which screen (multi-monitor)

    # Derived insights
    tags: List[str] = field(default_factory=list)
    related_files: List[str] = field(default_factory=list)
    confidence: float = 1.0  # How confident we are in the summary


class ScreenChangeDetector:
    """Detects significant changes in screen content to avoid redundant captures."""

    def __init__(self, threshold: float = 0.05):
        """
        Args:
            threshold: Fraction of pixels that must change (0.05 = 5%)
        """
        self.threshold = threshold
        self.last_hash = None
        self.last_image = None

    def has_significant_change(self, image) -> bool:
        """Check if image has changed significantly from last capture."""
        if self.last_image is None:
            self.last_image = image
            return True

        # Convert to grayscale for comparison
        if HAS_CV2:
            gray1 = cv2.cvtColor(np.array(self.last_image), cv2.COLOR_RGB2GRAY)
            gray2 = cv2.cvtColor(np.array(image), cv2.COLOR_RGB2GRAY)

            # Calculate absolute difference
            diff = cv2.absdiff(gray1, gray2)
            change_ratio = np.sum(diff > 30) / diff.size  # Pixels changed > threshold

            has_changed = change_ratio > self.threshold

            if has_changed:
                self.last_image = image

            return has_changed
        else:
            # Fallback: simple hash comparison
            current_hash = hashlib.md5(image.tobytes()).hexdigest()
            if self.last_hash != current_hash:
                self.last_hash = current_hash
                self.last_image = image
                return True
            return False


class OCRExtractor:
    """Extract text from screenshots using OCR."""

    def __init__(self):
        self.has_ocr = False
        self.ocr_engine = None

        # Try to load OCR engines
        try:
            import pytesseract
            self.ocr_engine = pytesseract
            self.has_ocr = True
        except ImportError:
            pass

    def extract_text(self, image_path: str) -> str:
        """Extract text from screenshot."""
        if not self.has_ocr or not HAS_PIL:
            return ""

        try:
            image = Image.open(image_path)

            # Preprocess for better OCR
            # Convert to grayscale
            image = image.convert('L')

            # Use pytesseract
            text = self.ocr_engine.image_to_string(image)

            return text.strip()
        except Exception as e:
            print(f"[OCR] Error extracting text: {e}")
            return ""


class VisionSummarizer:
    """Summarize screenshots using vision AI models."""

    def __init__(self, kernel: SemanticKernel = None):
        self.kernel = kernel
        self.has_vision = False

        # Check for available AI providers
        try:
            # Try to import vision-capable libraries
            import openai
            import anthropic
            self.has_vision = True
        except ImportError:
            pass

        # Check for local vision models (Ollama, etc.)
        try:
            import requests
            # Check if Ollama is available
            resp = requests.get('http://localhost:11434/api/tags', timeout=1)
            if resp.status_code == 200:
                self.has_ollama = True
                self.has_vision = True
        except:
            self.has_ollama = False

    def summarize(self, image_path: str, ocr_text: str = "") -> str:
        """Generate a summary of what's in the screenshot."""
        if not self.has_vision:
            # Fallback to OCR-based summary
            return self._ocr_summary(ocr_text)

        # For now, use OCR-based summary
        # TODO: Add vision model integration
        return self._ocr_summary(ocr_text)

    def _ocr_summary(self, ocr_text: str) -> str:
        """Generate summary from OCR text."""
        if not ocr_text:
            return "[No text content detected]"

        lines = ocr_text.split('\n')
        non_empty = [l.strip() for l in lines if l.strip()]

        if len(non_empty) <= 5:
            return f"Text content: {', '.join(non_empty[:3])}"
        else:
            return f"{len(non_empty)} lines of text visible, including: {non_empty[0][:50]}..."


class VisualMemoryCapture:
    """Main class for capturing and managing visual memory."""

    def __init__(
        self,
        kernel: SemanticKernel = None,
        storage_dir: str = None,
        capture_interval: int = 30,  # seconds
        change_threshold: float = 0.05,
        idle_timeout: int = 300,  # 5 minutes
    ):
        """
        Args:
            kernel: Semantic kernel for storing memories
            storage_dir: Directory to store screenshots
            capture_interval: Seconds between captures
            change_threshold: Fraction of screen that must change
            idle_timeout: Seconds of inactivity before pausing
        """
        self.kernel = kernel or SemanticKernel(enable_qt=False)
        self.storage_dir = Path(storage_dir or self.kernel.memory.db_path).parent / "visual_memory"
        self.storage_dir.mkdir(parents=True, exist_ok=True)

        self.capture_interval = capture_interval
        self.idle_timeout = idle_timeout

        # Components
        self.change_detector = ScreenChangeDetector(threshold=change_threshold)
        self.ocr_extractor = OCRExtractor()
        self.vision_summarizer = VisionSummarizer(self.kernel)

        # State
        self.running = False
        self.capture_thread = None
        self.last_activity = time.time()
        self.captures: deque[ScreenCapture] = deque(maxlen=1000)  # Keep last 1000

        # Activity tracking
        self._last_mouse_pos = None
        self._last_keys_pressed = []

        # Blacklist (apps/windows to never capture)
        self.blacklist = set([
            "password",
            "secret",
            "login",
            "auth",
            "credential",
            "2fa",
            "otp",
        ])

    def _get_active_window(self) -> tuple[str, str]:
        """Get the active window title and app name."""
        if sys.platform == 'win32':
            try:
                import ctypes.wintypes
                from ctypes import windll

                def get_window_title(hwnd):
                    length = windll.user32.GetWindowTextLengthW(hwnd) + 1
                    buff = ctypes.create_unicode_buffer(length)
                    windll.user32.GetWindowTextW(hwnd, buff, length)
                    return buff.value

                # Get foreground window
                hwnd = windll.user32.GetForegroundWindow()
                title = get_window_title(hwnd)

                # Try to get app name from window class
                class_name = ctypes.create_unicode_buffer(256)
                windll.user32.GetClassNameW(hwnd, class_name, 256)

                return title, class_name.value
            except:
                return "", ""
        else:
            return "", ""

    def _is_blacklisted(self, title: str, app_name: str) -> bool:
        """Check if window should be blacklisted."""
        combined = (title + " " + app_name).lower()
        return any(term in combined for term in self.blacklist)

    def _check_activity(self) -> bool:
        """Check if there's been recent user activity."""
        # Simple check: if it's been too long since last capture
        # TODO: Could add mouse/keyboard hook for more accurate detection
        return (time.time() - self.last_activity) < self.idle_timeout

    def _capture_screen(self) -> Optional[Image.Image]:
        """Capture current screen."""
        if not HAS_PIL:
            print("[Visual Memory] PIL not available, cannot capture screen")
            return None

        try:
            # Capture primary screen
            screenshot = ImageGrab.grab()
            return screenshot
        except Exception as e:
            print(f"[Visual Memory] Error capturing screen: {e}")
            return None

    def _save_capture(self, image: Image.Image, timestamp: float) -> Optional[ScreenCapture]:
        """Save screenshot and generate metadata."""
        # Create date directory
        date_str = datetime.fromtimestamp(timestamp).strftime("%Y-%m-%d")
        date_dir = self.storage_dir / date_str
        date_dir.mkdir(exist_ok=True)

        # Generate filename
        time_str = datetime.fromtimestamp(timestamp).strftime("%H-%M-%S")
        image_path = date_dir / f"{time_str}.png"
        thumbnail_path = date_dir / f"{time_str}_thumb.png"

        # Save full image
        try:
            image.save(image_path, "PNG", optimize=True)
        except Exception as e:
            print(f"[Visual Memory] Error saving image: {e}")
            return None

        # Create and save thumbnail
        try:
            thumb = image.copy()
            thumb.thumbnail((320, 240))
            thumb.save(thumbnail_path, "PNG", optimize=True)
        except Exception as e:
            print(f"[Visual Memory] Error creating thumbnail: {e}")
            thumbnail_path = ""

        # Get active window info
        title, app_name = self._get_active_window()

        # Generate summary
        ocr_text = ""
        summary = ""

        # Only OCR if not blacklisted
        if not self._is_blacklisted(title, app_name):
            ocr_text = self.ocr_extractor.extract_text(str(image_path))
            summary = self.vision_summarizer.summarize(str(image_path), ocr_text)

        # Create capture record
        capture = ScreenCapture(
            timestamp=timestamp,
            image_path=str(image_path),
            thumbnail_path=str(thumbnail_path) if thumbnail_path else "",
            summary=summary,
            ocr_text=ocr_text[:500],  # Limit stored OCR text
            active_window=title,
            app_name=app_name,
            hash=hashlib.md5(image.tobytes()).hexdigest(),
        )

        # Save metadata
        metadata_path = date_dir / f"{time_str}.json"
        try:
            with open(metadata_path, 'w') as f:
                json.dump({
                    'timestamp': capture.timestamp,
                    'summary': capture.summary,
                    'ocr_text': capture.ocr_text,
                    'active_window': capture.active_window,
                    'app_name': capture.app_name,
                    'hash': capture.hash,
                    'tags': capture.tags,
                    'related_files': capture.related_files,
                }, f, indent=2)
        except Exception as e:
            print(f"[Visual Memory] Error saving metadata: {e}")

        return capture

    def _store_in_kernel(self, capture: ScreenCapture):
        """Store capture in semantic kernel memory."""
        if not self.kernel:
            return

        try:
            content = f"Screenshot: {capture.summary}"

            # Store in memory
            self.kernel.memory.store(
                content=content,
                type="visual_memory",
                metadata={
                    'timestamp': capture.timestamp,
                    'image_path': capture.image_path,
                    'thumbnail_path': capture.thumbnail_path,
                    'active_window': capture.active_window,
                    'app_name': capture.app_name,
                    'tags': capture.tags,
                    'type': 'screenshot'
                }
            )

            # Emit event
            self.kernel.emit("visual_memory.captured", {
                'timestamp': capture.timestamp,
                'summary': capture.summary,
                'window': capture.active_window
            })

        except Exception as e:
            print(f"[Visual Memory] Error storing in kernel: {e}")

    def _capture_loop(self):
        """Main capture loop (runs in background thread)."""
        while self.running:
            try:
                # Check if user is active
                if not self._check_activity():
                    time.sleep(self.capture_interval)
                    continue

                # Capture screen
                image = self._capture_screen()
                if image is None:
                    time.sleep(self.capture_interval)
                    continue

                # Check for significant changes
                if not self.change_detector.has_significant_change(image):
                    time.sleep(self.capture_interval)
                    continue

                # Save capture
                capture = self._save_capture(image, time.time())
                if capture:
                    self.captures.append(capture)
                    self._store_in_kernel(capture)
                    print(f"[Visual Memory] Captured: {capture.summary}")

            except Exception as e:
                print(f"[Visual Memory] Error in capture loop: {e}")

            # Wait before next capture
            time.sleep(self.capture_interval)

    def start(self):
        """Start capturing visual memory."""
        if self.running:
            print("[Visual Memory] Already running")
            return

        self.running = True
        self.capture_thread = threading.Thread(target=self._capture_loop, daemon=True)
        self.capture_thread.start()
        print(f"[Visual Memory] Started (interval: {self.capture_interval}s)")

    def stop(self):
        """Stop capturing visual memory."""
        self.running = False
        if self.capture_thread:
            self.capture_thread.join(timeout=5)
        print("[Visual Memory] Stopped")

    def get_captures(self, limit: int = 100) -> List[ScreenCapture]:
        """Get recent captures."""
        captures = list(self.captures)
        captures.sort(key=lambda c: c.timestamp, reverse=True)
        return captures[:limit]

    def get_captures_in_range(self, start_time: float, end_time: float) -> List[ScreenCapture]:
        """Get captures within a time range."""
        return [
            c for c in self.captures
            if start_time <= c.timestamp <= end_time
        ]

    def query(self, query_text: str, limit: int = 10) -> List[ScreenCapture]:
        """Query captures by content."""
        query_lower = query_text.lower()

        results = []
        for capture in self.captures:
            # Search in summary, OCR text, window title
            if (query_lower in capture.summary.lower() or
                query_lower in capture.ocr_text.lower() or
                query_lower in capture.active_window.lower() or
                query_lower in capture.app_name.lower() or
                any(query_lower in tag.lower() for tag in capture.tags)):
                results.append(capture)

        results.sort(key=lambda c: c.timestamp, reverse=True)
        return results[:limit]


import sys
