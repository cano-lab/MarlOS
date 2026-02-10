import { Component, createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./DecisionLogger.css";

interface DecisionLoggerProps {
  isOpen: boolean;
  onClose: () => void;
  defaultProject?: string;
  defaultFilePath?: string;
  onDecisionLogged?: (id: string) => void;
}

const DecisionLogger: Component<DecisionLoggerProps> = (props) => {
  const [topic, setTopic] = createSignal("");
  const [choice, setChoice] = createSignal("");
  const [reasoning, setReasoning] = createSignal("");
  const [tags, setTags] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal(false);

  const handleSubmit = async (e: Event) => {
    e.preventDefault();

    if (!topic().trim() || !choice().trim() || !reasoning().trim()) {
      setError("Topic, choice, and reasoning are required.");
      return;
    }

    setSaving(true);
    setError(null);

    try {
      const tagList = tags()
        .split(",")
        .map((t) => t.trim())
        .filter((t) => t.length > 0);

      const id = await invoke<string>("proactive_log_decision", {
        topic: topic().trim(),
        choice: choice().trim(),
        reasoning: reasoning().trim(),
        project: props.defaultProject || null,
        filePath: props.defaultFilePath || null,
        tags: tagList.length > 0 ? tagList : null,
      });

      setSuccess(true);
      props.onDecisionLogged?.(id);

      // Reset form after short delay
      setTimeout(() => {
        setTopic("");
        setChoice("");
        setReasoning("");
        setTags("");
        setSuccess(false);
        props.onClose();
      }, 1500);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleClose = () => {
    if (!saving()) {
      setTopic("");
      setChoice("");
      setReasoning("");
      setTags("");
      setError(null);
      setSuccess(false);
      props.onClose();
    }
  };

  return (
    <Show when={props.isOpen}>
      <div class="decision-logger-overlay" onClick={handleClose}>
        <div class="decision-logger-modal" onClick={(e) => e.stopPropagation()}>
          <div class="modal-header">
            <h3>Log Decision</h3>
            <button class="close-btn" onClick={handleClose} disabled={saving()}>
              ×
            </button>
          </div>

          <Show when={success()}>
            <div class="success-message">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2" />
                <path d="M8 12l3 3 5-6" stroke="currentColor" stroke-width="2" fill="none" />
              </svg>
              Decision logged successfully!
            </div>
          </Show>

          <Show when={!success()}>
            <form onSubmit={handleSubmit}>
              <div class="form-group">
                <label for="topic">Topic *</label>
                <input
                  id="topic"
                  type="text"
                  value={topic()}
                  onInput={(e) => setTopic(e.currentTarget.value)}
                  placeholder="What is this decision about?"
                  disabled={saving()}
                  autofocus
                />
              </div>

              <div class="form-group">
                <label for="choice">Choice *</label>
                <input
                  id="choice"
                  type="text"
                  value={choice()}
                  onInput={(e) => setChoice(e.currentTarget.value)}
                  placeholder="What did you decide?"
                  disabled={saving()}
                />
              </div>

              <div class="form-group">
                <label for="reasoning">Reasoning *</label>
                <textarea
                  id="reasoning"
                  value={reasoning()}
                  onInput={(e) => setReasoning(e.currentTarget.value)}
                  placeholder="Why did you make this choice? What alternatives did you consider?"
                  rows={4}
                  disabled={saving()}
                />
              </div>

              <div class="form-group">
                <label for="tags">Tags</label>
                <input
                  id="tags"
                  type="text"
                  value={tags()}
                  onInput={(e) => setTags(e.currentTarget.value)}
                  placeholder="architecture, database, performance (comma-separated)"
                  disabled={saving()}
                />
              </div>

              <Show when={props.defaultProject}>
                <div class="context-info">
                  <span class="context-label">Project:</span>
                  <span class="context-value">{props.defaultProject}</span>
                </div>
              </Show>

              <Show when={error()}>
                <div class="error-message">{error()}</div>
              </Show>

              <div class="form-actions">
                <button
                  type="button"
                  class="cancel-btn"
                  onClick={handleClose}
                  disabled={saving()}
                >
                  Cancel
                </button>
                <button type="submit" class="submit-btn" disabled={saving()}>
                  {saving() ? "Saving..." : "Log Decision"}
                </button>
              </div>
            </form>
          </Show>

          <div class="modal-footer">
            <p class="footer-hint">
              Decisions are stored in your memory and will be surfaced when you revisit related topics.
            </p>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default DecisionLogger;
