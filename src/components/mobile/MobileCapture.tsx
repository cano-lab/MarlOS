import { Component, createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./MobileCapture.css";

type CaptureMode = "text" | "photo" | "voice";

interface MobileCaptureProps {
  onCapture?: (id: string) => void;
  onClose?: () => void;
}

const MobileCapture: Component<MobileCaptureProps> = (props) => {
  const [mode, setMode] = createSignal<CaptureMode>("text");
  const [content, setContent] = createSignal("");
  const [tags, setTags] = createSignal("");
  const [title, setTitle] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [photoPreview, setPhotoPreview] = createSignal<string | null>(null);
  const [isRecording, setIsRecording] = createSignal(false);

  const handleSave = async () => {
    if (!content().trim() && !photoPreview()) return;

    setSaving(true);
    try {
      const tagList = tags()
        .split(",")
        .map((t) => t.trim())
        .filter((t) => t.length > 0);

      // Create semantic object for the capture
      const captureData = {
        type: mode(),
        title: title().trim() || generateTitle(),
        content: content().trim(),
        photo: photoPreview(),
        tags: tagList,
        captured_at: new Date().toISOString(),
        source: "mobile_capture",
      };

      const id = await invoke<string>("quick_capture", {
        captureType: mode(),
        content: JSON.stringify(captureData),
        tags: tagList,
      });

      props.onCapture?.(id);
      resetForm();
    } catch (e) {
      console.error("Failed to save capture:", e);
    } finally {
      setSaving(false);
    }
  };

  const generateTitle = () => {
    const now = new Date();
    const prefix = mode() === "photo" ? "Photo" : mode() === "voice" ? "Voice Note" : "Note";
    return `${prefix} - ${now.toLocaleDateString()} ${now.toLocaleTimeString()}`;
  };

  const resetForm = () => {
    setContent("");
    setTags("");
    setTitle("");
    setPhotoPreview(null);
  };

  const handlePhotoCapture = async () => {
    try {
      // Use Tauri camera plugin when available
      const photo = await invoke<string>("capture_photo");
      setPhotoPreview(photo);
      setMode("photo");
    } catch (e) {
      console.error("Failed to capture photo:", e);
      // Fallback to file picker
      const input = document.createElement("input");
      input.type = "file";
      input.accept = "image/*";
      input.capture = "environment";
      input.onchange = (e) => {
        const file = (e.target as HTMLInputElement).files?.[0];
        if (file) {
          const reader = new FileReader();
          reader.onload = () => {
            setPhotoPreview(reader.result as string);
            setMode("photo");
          };
          reader.readAsDataURL(file);
        }
      };
      input.click();
    }
  };

  const toggleVoiceRecording = async () => {
    if (isRecording()) {
      setIsRecording(false);
      // Stop recording and transcribe
      try {
        const transcript = await invoke<string>("stop_voice_recording");
        setContent((prev) => prev + (prev ? "\n" : "") + transcript);
      } catch (e) {
        console.error("Failed to transcribe:", e);
      }
    } else {
      setIsRecording(true);
      setMode("voice");
      try {
        await invoke("start_voice_recording");
      } catch (e) {
        console.error("Failed to start recording:", e);
        setIsRecording(false);
      }
    }
  };

  return (
    <div class="mobile-capture">
      <div class="capture-header">
        <h2>Quick Capture</h2>
        <Show when={props.onClose}>
          <button class="close-btn" onClick={props.onClose}>
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
              <path d="M6 6l12 12M6 18L18 6" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>
        </Show>
      </div>

      {/* Mode Selector */}
      <div class="mode-selector">
        <button
          class={`mode-btn ${mode() === "text" ? "active" : ""}`}
          onClick={() => setMode("text")}
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
            <path d="M3 4h14v2H3V4zm0 5h10v2H3V9zm0 5h14v2H3v-2z" />
          </svg>
          Text
        </button>
        <button
          class={`mode-btn ${mode() === "photo" ? "active" : ""}`}
          onClick={handlePhotoCapture}
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
            <path d="M4 5h3l1-2h4l1 2h3a1 1 0 011 1v9a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1z" fill="none" stroke="currentColor" stroke-width="1.5" />
            <circle cx="10" cy="10" r="3" fill="none" stroke="currentColor" stroke-width="1.5" />
          </svg>
          Photo
        </button>
        <button
          class={`mode-btn ${mode() === "voice" || isRecording() ? "active" : ""} ${isRecording() ? "recording" : ""}`}
          onClick={toggleVoiceRecording}
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
            <rect x="7" y="2" width="6" height="10" rx="3" fill="none" stroke="currentColor" stroke-width="1.5" />
            <path d="M4 9a6 6 0 0012 0M10 15v3" stroke="currentColor" stroke-width="1.5" fill="none" />
          </svg>
          {isRecording() ? "Stop" : "Voice"}
        </button>
      </div>

      {/* Photo Preview */}
      <Show when={photoPreview()}>
        <div class="photo-preview">
          <img src={photoPreview()!} alt="Captured" />
          <button class="remove-photo" onClick={() => setPhotoPreview(null)}>
            <svg width="16" height="16" viewBox="0 0 16 16">
              <path d="M4 4l8 8M4 12l8-8" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>
        </div>
      </Show>

      {/* Recording Indicator */}
      <Show when={isRecording()}>
        <div class="recording-indicator">
          <span class="recording-dot" />
          Recording... Tap Voice to stop
        </div>
      </Show>

      {/* Title Input */}
      <input
        type="text"
        class="capture-title"
        placeholder="Title (optional)"
        value={title()}
        onInput={(e) => setTitle(e.currentTarget.value)}
      />

      {/* Content Area */}
      <textarea
        class="capture-content"
        placeholder={
          mode() === "photo"
            ? "Add notes about this photo..."
            : mode() === "voice"
            ? "Voice transcript will appear here..."
            : "What's on your mind?"
        }
        value={content()}
        onInput={(e) => setContent(e.currentTarget.value)}
        rows={6}
      />

      {/* Tags Input */}
      <input
        type="text"
        class="capture-tags"
        placeholder="Tags (comma-separated)"
        value={tags()}
        onInput={(e) => setTags(e.currentTarget.value)}
      />

      {/* Quick Tags */}
      <div class="quick-tags">
        {["idea", "todo", "research", "question", "decision"].map((tag) => (
          <button
            class={`quick-tag ${tags().includes(tag) ? "selected" : ""}`}
            onClick={() => {
              const currentTags = tags()
                .split(",")
                .map((t) => t.trim())
                .filter((t) => t.length > 0);
              if (currentTags.includes(tag)) {
                setTags(currentTags.filter((t) => t !== tag).join(", "));
              } else {
                setTags([...currentTags, tag].join(", "));
              }
            }}
          >
            {tag}
          </button>
        ))}
      </div>

      {/* Save Button */}
      <button
        class="save-btn"
        onClick={handleSave}
        disabled={saving() || (!content().trim() && !photoPreview())}
      >
        {saving() ? "Saving..." : "Save to Memory"}
      </button>
    </div>
  );
};

export default MobileCapture;
