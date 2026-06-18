import { Component, createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { Reference } from "./ReferenceLibrary";
import "./ReferenceImport.css";

interface ReferenceImportProps {
  onClose: () => void;
  onImported: () => void;
}

const ReferenceImport: Component<ReferenceImportProps> = (props) => {
  const [bibtexText, setBibtexText] = createSignal("");
  const [importing, setImporting] = createSignal(false);
  const [result, setResult] = createSignal<{ count: number; error?: string } | null>(null);

  const handleFileSelect = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [
          { name: "BibTeX", extensions: ["bib", "bibtex"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (selected && typeof selected === "string") {
        const { readTextFile } = await import("@tauri-apps/plugin-fs");
        const content = await readTextFile(selected);
        setBibtexText(content);
      }
    } catch (e) {
      console.error("Failed to read file:", e);
    }
  };

  const handleImport = async () => {
    if (!bibtexText().trim()) return;
    setImporting(true);
    try {
      const refs = await invoke<Reference[]>("ref_import_bibtex", { bibtex: bibtexText() });
      setResult({ count: refs.length });
      setTimeout(() => {
        props.onImported();
      }, 1500);
    } catch (e) {
      setResult({ count: 0, error: `${e}` });
    } finally {
      setImporting(false);
    }
  };

  return (
    <div class="refimport-overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="refimport-modal">
        <h3>Import BibTeX</h3>
        <p>Paste BibTeX content or select a .bib file.</p>

        <div class="refimport-actions-top">
          <button class="refimport-file-btn" onClick={handleFileSelect}>
            Select .bib file
          </button>
        </div>

        <textarea
          class="refimport-textarea"
          placeholder={"@article{key,\n  title = {Paper Title},\n  author = {Author Name},\n  year = {2024},\n  ..."}
          value={bibtexText()}
          onInput={(e) => setBibtexText(e.currentTarget.value)}
          rows={12}
        />

        <Show when={result()}>
          <div class={`refimport-result ${result()!.error ? "error" : "success"}`}>
            {result()!.error
              ? `Error: ${result()!.error}`
              : `Successfully imported ${result()!.count} references!`
            }
          </div>
        </Show>

        <div class="refimport-actions">
          <button onClick={props.onClose}>Cancel</button>
          <button
            class="refimport-btn-primary"
            onClick={handleImport}
            disabled={importing() || !bibtexText().trim()}
          >
            {importing() ? "Importing..." : "Import"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default ReferenceImport;
