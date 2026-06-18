import { Component, createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { Reference } from "./ReferenceLibrary";
import "./ReferenceDetail.css";

interface ReferenceDetailProps {
  reference: Reference;
  onClose: () => void;
  onDelete: (id: string) => void;
  onStatusChange: (id: string, status: string) => void;
  onUpdate: () => void;
  onOpenPdf?: (path: string) => void;
}

const STATUSES = [
  { value: "unread", label: "Unread" },
  { value: "reading", label: "Reading" },
  { value: "read", label: "Read" },
  { value: "archived", label: "Archived" },
];

const ReferenceDetail: Component<ReferenceDetailProps> = (props) => {
  const [_editing, _setEditing] = createSignal(false);
  const [notes, setNotes] = createSignal(props.reference.notes || "");
  const [showBibtex, setShowBibtex] = createSignal(false);
  const [bibtex, setBibtex] = createSignal("");

  const ref = () => props.reference;

  const handleSaveNotes = async () => {
    try {
      const updated = { ...ref(), notes: notes() };
      await invoke("ref_update", { reference: updated });
      props.onUpdate();
    } catch (e) {
      console.error("Failed to save notes:", e);
    }
  };

  const handleShowBibtex = async () => {
    try {
      const bib = await invoke<string>("ref_export_bibtex", { ids: [ref().id] });
      setBibtex(bib);
      setShowBibtex(true);
    } catch (e) {
      console.error("Failed to get BibTeX:", e);
    }
  };

  const handleCopyBibtex = async () => {
    await navigator.clipboard.writeText(bibtex());
  };

  const handleAttachPdf = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (selected && typeof selected === "string") {
        await invoke("ref_attach_pdf", { id: ref().id, pdfPath: selected });
        props.onUpdate();
      }
    } catch (e) {
      console.error("Failed to attach PDF:", e);
    }
  };

  const formatDate = (iso: string) => new Date(iso).toLocaleDateString();

  return (
    <div class="refdetail">
      <div class="refdetail-header">
        <h3>Details</h3>
        <button class="refdetail-close" onClick={props.onClose}>x</button>
      </div>

      <div class="refdetail-body">
        <h4 class="refdetail-title">{ref().title}</h4>

        <div class="refdetail-authors">
          {ref().authors.join(", ") || "Unknown authors"}
        </div>

        <div class="refdetail-meta">
          {ref().year && <span>{ref().year}</span>}
          {ref().journal && <span class="refdetail-journal">{ref().journal}</span>}
          {ref().volume && <span>Vol. {ref().volume}</span>}
          {ref().issue && <span>({ref().issue})</span>}
          {ref().pages && <span>pp. {ref().pages}</span>}
        </div>

        {/* Status */}
        <div class="refdetail-section">
          <label>Status</label>
          <div class="refdetail-status-btns">
            {STATUSES.map((s) => (
              <button
                class="refdetail-status-btn"
                classList={{ active: ref().reading_status === s.value }}
                onClick={() => props.onStatusChange(ref().id, s.value)}
              >
                {s.label}
              </button>
            ))}
          </div>
        </div>

        {/* Identifiers */}
        <div class="refdetail-section">
          <label>Identifiers</label>
          <div class="refdetail-ids">
            {ref().doi && (
              <div class="refdetail-id">
                <span class="refdetail-id-label">DOI</span>
                <span class="refdetail-id-value">{ref().doi}</span>
              </div>
            )}
            {ref().isbn && (
              <div class="refdetail-id">
                <span class="refdetail-id-label">ISBN</span>
                <span class="refdetail-id-value">{ref().isbn}</span>
              </div>
            )}
            <div class="refdetail-id">
              <span class="refdetail-id-label">Cite Key</span>
              <span class="refdetail-id-value">@{ref().cite_key}</span>
            </div>
          </div>
        </div>

        {/* Abstract */}
        <Show when={ref().abstract_text}>
          <div class="refdetail-section">
            <label>Abstract</label>
            <div class="refdetail-abstract">{ref().abstract_text}</div>
          </div>
        </Show>

        {/* Keywords */}
        <Show when={ref().keywords.length > 0}>
          <div class="refdetail-section">
            <label>Keywords</label>
            <div class="refdetail-keywords">
              {ref().keywords.map((k) => (
                <span class="refdetail-keyword">{k}</span>
              ))}
            </div>
          </div>
        </Show>

        {/* PDF */}
        <div class="refdetail-section">
          <label>PDF</label>
          <Show
            when={ref().pdf_path}
            fallback={<button class="refdetail-action" onClick={handleAttachPdf}>Attach PDF</button>}
          >
            <div class="refdetail-pdf">
              <span class="refdetail-pdf-name">{ref().pdf_path!.split(/[/\\]/).pop()}</span>
              <button
                class="refdetail-action"
                onClick={() => props.onOpenPdf?.(ref().pdf_path!)}
              >
                Open
              </button>
            </div>
          </Show>
        </div>

        {/* Notes */}
        <div class="refdetail-section">
          <label>Notes</label>
          <textarea
            class="refdetail-notes"
            value={notes()}
            onInput={(e) => setNotes(e.currentTarget.value)}
            onBlur={handleSaveNotes}
            placeholder="Add notes about this reference..."
            rows={4}
          />
        </div>

        {/* Actions */}
        <div class="refdetail-actions">
          <button class="refdetail-action" onClick={handleShowBibtex}>BibTeX</button>
          {ref().url && (
            <button
              class="refdetail-action"
              onClick={() => window.open(ref().url!, "_blank")}
            >
              Open URL
            </button>
          )}
          <button
            class="refdetail-action refdetail-action-danger"
            onClick={() => {
              if (confirm("Delete this reference?")) props.onDelete(ref().id);
            }}
          >
            Delete
          </button>
        </div>

        <div class="refdetail-footer">
          Added {formatDate(ref().added_at)} | Type: {ref().ref_type}
        </div>
      </div>

      {/* BibTeX modal */}
      <Show when={showBibtex()}>
        <div class="refdetail-bibtex-overlay" onClick={() => setShowBibtex(false)}>
          <div class="refdetail-bibtex-modal" onClick={(e) => e.stopPropagation()}>
            <pre>{bibtex()}</pre>
            <div class="refdetail-bibtex-actions">
              <button onClick={handleCopyBibtex}>Copy</button>
              <button onClick={() => setShowBibtex(false)}>Close</button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default ReferenceDetail;
