import { Component, createSignal, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { Widget } from "./PlanSpace";
import "./FileWidget.css";

interface FileWidgetProps {
  widget: Widget;
  onUpdate: (updates: Partial<Widget>) => void;
}

const FileWidget: Component<FileWidgetProps> = (props) => {
  const [filePath, setFilePath] = createSignal("");
  const [isImage, setIsImage] = createSignal(false);
  const [isPicking, setIsPicking] = createSignal(false);

  onMount(() => {
    if (props.widget.data.type === "file") {
      setFilePath(props.widget.data.filePath || "");
      setIsImage(props.widget.data.isImage || false);
    }
  });

  const pickFile = async () => {
    try {
      setIsPicking(true);
      const selected = await invoke<string | null>("pick_file");

      if (selected) {
        setFilePath(selected);
        const imageExt = [".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".svg"];
        const isImg = imageExt.some(ext => selected.toLowerCase().endsWith(ext));
        setIsImage(isImg);

        saveSettings({ filePath: selected, isImage: isImg });
      }
    } catch (e) {
      console.error("Failed to pick file:", e);
    } finally {
      setIsPicking(false);
    }
  };

  const openFile = async () => {
    const path = filePath();
    if (!path) return;

    try {
      await invoke("open_file_path", { path });
    } catch (e) {
      console.error("Failed to open file:", e);
    }
  };

  const saveSettings = (settings: Record<string, any>) => {
    const updates: Partial<Widget> = {
      data: {
        type: "file",
        filePath: filePath(),
        isImage: isImage(),
        ...settings,
      },
      lastTouched: new Date(),
    };
    props.onUpdate(updates);
  };

  const getFileName = () => {
    const path = filePath();
    if (!path) return "No file selected";
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1];
  };

  const getFileIcon = () => {
    if (isImage()) return "🖼";
    const path = filePath().toLowerCase();
    if (path.endsWith(".pdf")) return "📄";
    if (path.endsWith(".doc") || path.endsWith(".docx")) return "📝";
    if (path.endsWith(".xls") || path.endsWith(".xlsx")) return "📊";
    if (path.endsWith(".ppt") || path.endsWith(".pptx")) return "📊";
    if (path.endsWith(".txt")) return "📃";
    if (path.endsWith(".zip") || path.endsWith(".rar") || path.endsWith(".7z")) return "🗜";
    if (path.endsWith(".mp3") || path.endsWith(".wav") || path.endsWith(".flac")) return "🎵";
    if (path.endsWith(".mp4") || path.endsWith(".mkv") || path.endsWith(".webm")) return "🎬";
    if (path.endsWith(".code") || path.endsWith(".js") || path.endsWith(".ts") || path.endsWith(".py") || path.endsWith(".rs")) return "💻";
    return "📁";
  };

  return (
    <div class="file-widget">
      <Show when={filePath()}>
        <div class="file-content" onClick={openFile} title="Click to open file">
          <Show when={isImage()}>
            <div class="file-image-preview">
              <img src={`file://${filePath()}`} alt="Preview" />
            </div>
          </Show>
          <Show when={!isImage()}>
            <div class="file-icon">
              {getFileIcon()}
            </div>
          </Show>
          <div class="file-name">
            {getFileName()}
          </div>
        </div>
      </Show>

      <Show when={!filePath()}>
        <div class="file-empty" onClick={pickFile}>
          <div class="file-empty-icon">📎</div>
          <div class="file-empty-text">Click to select file</div>
        </div>
      </Show>

      <div class="file-actions">
        <button
          class="file-pick-btn"
          onClick={pickFile}
          disabled={isPicking()}
          title="Change file"
        >
          {isPicking() ? "..." : filePath() ? "Change" : "Select"}
        </button>
        <Show when={filePath()}>
          <button
            class="file-clear-btn"
            onClick={() => {
              setFilePath("");
              setIsImage(false);
              saveSettings({ filePath: "", isImage: false });
            }}
            title="Clear"
          >
            ✕
          </button>
        </Show>
      </div>
    </div>
  );
};

export default FileWidget;
