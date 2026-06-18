import { Component, createSignal, onMount, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./VersionHistory.css";

interface VersionSummary {
  id: string;
  document_path: string;
  word_count: number;
  saved_at: string;
  label: string | null;
}

interface DocumentVersion {
  id: string;
  document_path: string;
  content: string;
  word_count: number;
  saved_at: string;
  label: string | null;
}

interface DiffChunk {
  kind: string;
  lines: string[];
}

interface VersionDiff {
  added_lines: number;
  removed_lines: number;
  changes: DiffChunk[];
}

interface VersionHistoryProps {
  documentPath: string;
  currentContent: string;
  onRestore: (content: string) => void;
  onClose: () => void;
}

const VersionHistory: Component<VersionHistoryProps> = (props) => {
  const [versions, setVersions] = createSignal<VersionSummary[]>([]);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [diff, setDiff] = createSignal<VersionDiff | null>(null);
  const [selectedContent, setSelectedContent] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);

  onMount(async () => {
    await loadVersions();
  });

  const loadVersions = async () => {
    try {
      const list = await invoke<VersionSummary[]>("version_list", {
        documentPath: props.documentPath,
      });
      setVersions(list);
    } catch (e) {
      console.error("Failed to load versions:", e);
    }
  };

  const selectVersion = async (id: string) => {
    setSelectedId(id);
    setLoading(true);
    try {
      const [ver, d] = await Promise.all([
        invoke<DocumentVersion>("version_get", { versionId: id }),
        invoke<VersionDiff>("version_diff_current", {
          versionId: id,
          currentContent: props.currentContent,
        }),
      ]);
      setSelectedContent(ver.content);
      setDiff(d);
    } catch (e) {
      console.error("Failed to load version:", e);
    } finally {
      setLoading(false);
    }
  };

  const restoreVersion = () => {
    const content = selectedContent();
    if (content !== null) {
      props.onRestore(content);
    }
  };

  const formatDate = (isoString: string) => {
    const d = new Date(isoString);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 7) return `${days}d ago`;
    return d.toLocaleDateString();
  };

  return (
    <div class="vh-overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="vh-panel">
        <div class="vh-header">
          <h3>Version History</h3>
          <button class="vh-close" onClick={props.onClose}>x</button>
        </div>

        <div class="vh-body">
          <div class="vh-list">
            <div class="vh-list-header">
              <span>{versions().length} versions</span>
            </div>
            <Show
              when={versions().length > 0}
              fallback={<div class="vh-empty">No versions saved yet. Versions are created each time you save.</div>}
            >
              <For each={versions()}>
                {(ver) => (
                  <button
                    class="vh-item"
                    classList={{ "vh-item-selected": selectedId() === ver.id }}
                    onClick={() => selectVersion(ver.id)}
                  >
                    <div class="vh-item-time">{formatDate(ver.saved_at)}</div>
                    <div class="vh-item-meta">
                      <span>{ver.word_count.toLocaleString()} words</span>
                      {ver.label && <span class="vh-item-label">{ver.label}</span>}
                    </div>
                  </button>
                )}
              </For>
            </Show>
          </div>

          <div class="vh-diff">
            <Show
              when={selectedId()}
              fallback={<div class="vh-diff-empty">Select a version to see changes</div>}
            >
              <Show when={!loading()} fallback={<div class="vh-diff-empty">Loading...</div>}>
                <Show when={diff()}>
                  <div class="vh-diff-header">
                    <span class="vh-diff-added">+{diff()!.added_lines} added</span>
                    <span class="vh-diff-removed">-{diff()!.removed_lines} removed</span>
                    <button class="vh-restore-btn" onClick={restoreVersion}>
                      Restore this version
                    </button>
                  </div>
                  <div class="vh-diff-content">
                    <For each={diff()!.changes}>
                      {(chunk) => (
                        <div class={`vh-chunk vh-chunk-${chunk.kind}`}>
                          <For each={chunk.lines}>
                            {(line) => (
                              <div class="vh-line">
                                <span class="vh-line-marker">
                                  {chunk.kind === "add" ? "+" : chunk.kind === "remove" ? "-" : " "}
                                </span>
                                <span class="vh-line-text">{line || " "}</span>
                              </div>
                            )}
                          </For>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </Show>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
};

export default VersionHistory;
