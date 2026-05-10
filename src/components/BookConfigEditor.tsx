import { Component, createSignal, createEffect, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import {
  typesetterService,
  BookConfig,
} from "../services/typesetter-service";
import "./BookConfigEditor.css";

interface BookConfigEditorProps {
  /** Path the user originally used to load the book (file or dir). */
  bookPath: string;
  /** Current loaded config — the form initial state. */
  config: BookConfig;
  /** Called after a successful save with the resolved book.toml path. */
  onSaved: (tomlPath: string) => void;
  onClose: () => void;
}

const TRIM_PRESETS = [
  { value: "6x9", label: "6 × 9 in (KDP trade paperback)" },
  { value: "5.5x8.5", label: "5.5 × 8.5 in (digest)" },
  { value: "5x8", label: "5 × 8 in (mass market)" },
];

const TYPOGRAPHY_PRESETS = [
  {
    label: "Dense (doc default — 11pt / 14pt leading, 6×9 with KDP margins)",
    body_size_pt: 11.0,
    body_leading_pt: 14.0,
    margins: { inside: 1.0, outside: 0.875, top: 0.75, bottom: 0.75 },
  },
  {
    label: "Loose (11pt / 17pt leading — ~25% more pages)",
    body_size_pt: 11.0,
    body_leading_pt: 17.0,
    margins: { inside: 1.0, outside: 0.875, top: 0.75, bottom: 0.75 },
  },
  {
    label: "Trade paperback (12pt / 15.5pt leading — typical novel)",
    body_size_pt: 12.0,
    body_leading_pt: 15.5,
    margins: { inside: 1.0, outside: 0.875, top: 0.75, bottom: 0.75 },
  },
  {
    label: "Generous (12pt / 16pt, wider margins — airy literary feel)",
    body_size_pt: 12.0,
    body_leading_pt: 16.0,
    margins: { inside: 1.0, outside: 1.0, top: 0.9, bottom: 0.9 },
  },
];

const BookConfigEditor: Component<BookConfigEditorProps> = (props) => {
  // Local form state — copied from props on mount and on prop change.
  const [draft, setDraft] = createSignal<BookConfig>(structuredClone(props.config));
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);

  createEffect(() => {
    setDraft(structuredClone(props.config));
    setInfo(null);
    setError(null);
  });

  const update = (mutator: (d: BookConfig) => void) => {
    setDraft((prev) => {
      const next = structuredClone(prev);
      mutator(next);
      return next;
    });
  };

  const applyPreset = (preset: typeof TYPOGRAPHY_PRESETS[number]) => {
    update((d) => {
      d.typography.body_size_pt = preset.body_size_pt;
      d.typography.body_leading_pt = preset.body_leading_pt;
      d.trim.margins_in = { ...preset.margins };
    });
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      const path = await typesetterService.saveBook(props.bookPath, draft());
      setInfo(`Saved to ${path}`);
      props.onSaved(path);
    } catch (e) {
      setError(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const reset = () => {
    setDraft(structuredClone(props.config));
    setInfo(null);
    setError(null);
  };

  const pickCover = async (which: "front" | "back") => {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: "Image", extensions: ["jpg", "jpeg", "png", "tiff", "tif", "webp"] },
        ],
      });
      if (typeof picked === "string") {
        update((d) => {
          if (which === "front") d.book.cover_image = picked;
          else d.book.back_cover_image = picked;
        });
      }
    } catch (e) {
      setError(`File picker failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return (
    <div class="book-config-editor">
      <div class="bce-header">
        <h3>Book Settings</h3>
        <button class="bce-close" onClick={props.onClose} title="Close">
          ✕
        </button>
      </div>

      <div class="bce-body">
        <section class="bce-section">
          <h4>Metadata</h4>
          <label class="bce-field">
            <span>Title</span>
            <input
              type="text"
              value={draft().book.title}
              onInput={(e) => update((d) => (d.book.title = e.currentTarget.value))}
            />
          </label>
          <label class="bce-field">
            <span>Subtitle</span>
            <input
              type="text"
              value={draft().book.subtitle}
              onInput={(e) => update((d) => (d.book.subtitle = e.currentTarget.value))}
            />
          </label>
          <label class="bce-field">
            <span>Author</span>
            <input
              type="text"
              value={draft().book.author}
              onInput={(e) => update((d) => (d.book.author = e.currentTarget.value))}
            />
          </label>
          <div class="bce-row">
            <label class="bce-field bce-field-narrow">
              <span>ISBN</span>
              <input
                type="text"
                value={draft().book.isbn}
                onInput={(e) => update((d) => (d.book.isbn = e.currentTarget.value))}
              />
            </label>
            <label class="bce-field bce-field-narrow">
              <span>Language</span>
              <input
                type="text"
                value={draft().book.language ?? "en"}
                onInput={(e) =>
                  update((d) => (d.book.language = e.currentTarget.value))
                }
              />
            </label>
          </div>
        </section>

        <section class="bce-section">
          <h4>Covers</h4>
          <p class="bce-help">
            Image files for the front and back cover. JPG/PNG/TIFF/WebP. Each
            renders as a full-bleed page at the start/end of the book.
          </p>
          <div class="bce-cover-row">
            <span class="bce-cover-label">Front:</span>
            <span class="bce-cover-path" title={draft().book.cover_image ?? ""}>
              {draft().book.cover_image || "(none)"}
            </span>
            <button class="bce-btn" onClick={() => pickCover("front")}>Browse...</button>
            <Show when={draft().book.cover_image}>
              <button
                class="bce-btn"
                onClick={() => update((d) => (d.book.cover_image = null))}
                title="Clear"
              >
                ✕
              </button>
            </Show>
          </div>
          <div class="bce-cover-row">
            <span class="bce-cover-label">Back:</span>
            <span class="bce-cover-path" title={draft().book.back_cover_image ?? ""}>
              {draft().book.back_cover_image || "(none)"}
            </span>
            <button class="bce-btn" onClick={() => pickCover("back")}>Browse...</button>
            <Show when={draft().book.back_cover_image}>
              <button
                class="bce-btn"
                onClick={() => update((d) => (d.book.back_cover_image = null))}
                title="Clear"
              >
                ✕
              </button>
            </Show>
          </div>
        </section>

        <section class="bce-section">
          <h4>Typography presets</h4>
          <p class="bce-help">
            Quick-set body size, leading, and margins. You can fine-tune below.
          </p>
          <div class="bce-presets">
            {TYPOGRAPHY_PRESETS.map((preset) => (
              <button class="bce-preset-btn" onClick={() => applyPreset(preset)}>
                {preset.label}
              </button>
            ))}
          </div>
        </section>

        <section class="bce-section">
          <h4>Typography</h4>
          <label class="bce-field">
            <span>Body font</span>
            <input
              type="text"
              value={draft().typography.body_font}
              onInput={(e) =>
                update((d) => (d.typography.body_font = e.currentTarget.value))
              }
            />
          </label>
          <div class="bce-row">
            <label class="bce-field bce-field-narrow">
              <span>Size (pt)</span>
              <input
                type="number"
                step="0.5"
                value={draft().typography.body_size_pt}
                onInput={(e) =>
                  update(
                    (d) =>
                      (d.typography.body_size_pt =
                        parseFloat(e.currentTarget.value) || 0)
                  )
                }
              />
            </label>
            <label class="bce-field bce-field-narrow">
              <span>Leading (pt)</span>
              <input
                type="number"
                step="0.5"
                value={draft().typography.body_leading_pt}
                onInput={(e) =>
                  update(
                    (d) =>
                      (d.typography.body_leading_pt =
                        parseFloat(e.currentTarget.value) || 0)
                  )
                }
              />
            </label>
          </div>
        </section>

        <section class="bce-section">
          <h4>Trim & margins</h4>
          <label class="bce-field">
            <span>Trim size</span>
            <select
              value={draft().trim.size}
              onChange={(e) =>
                update((d) => (d.trim.size = e.currentTarget.value))
              }
            >
              {TRIM_PRESETS.map((p) => (
                <option value={p.value}>{p.label}</option>
              ))}
            </select>
          </label>
          <p class="bce-help">All margins in inches. Inside = spine side; outside = page edge.</p>
          <div class="bce-row">
            <label class="bce-field bce-field-narrow">
              <span>Inside</span>
              <input
                type="number"
                step="0.05"
                value={draft().trim.margins_in.inside}
                onInput={(e) =>
                  update(
                    (d) =>
                      (d.trim.margins_in.inside =
                        parseFloat(e.currentTarget.value) || 0)
                  )
                }
              />
            </label>
            <label class="bce-field bce-field-narrow">
              <span>Outside</span>
              <input
                type="number"
                step="0.05"
                value={draft().trim.margins_in.outside}
                onInput={(e) =>
                  update(
                    (d) =>
                      (d.trim.margins_in.outside =
                        parseFloat(e.currentTarget.value) || 0)
                  )
                }
              />
            </label>
          </div>
          <div class="bce-row">
            <label class="bce-field bce-field-narrow">
              <span>Top</span>
              <input
                type="number"
                step="0.05"
                value={draft().trim.margins_in.top}
                onInput={(e) =>
                  update(
                    (d) =>
                      (d.trim.margins_in.top =
                        parseFloat(e.currentTarget.value) || 0)
                  )
                }
              />
            </label>
            <label class="bce-field bce-field-narrow">
              <span>Bottom</span>
              <input
                type="number"
                step="0.05"
                value={draft().trim.margins_in.bottom}
                onInput={(e) =>
                  update(
                    (d) =>
                      (d.trim.margins_in.bottom =
                        parseFloat(e.currentTarget.value) || 0)
                  )
                }
              />
            </label>
          </div>
        </section>

        <section class="bce-section">
          <h4>Files (read-only)</h4>
          <p class="bce-help">
            Edit `book.toml` directly to reorder or add files. Order is reading order.
          </p>
          <ul class="bce-files">
            {draft().files.map((f) => (
              <li>{f}</li>
            ))}
          </ul>
        </section>

        <Show when={error()}>
          <div class="bce-error">{error()}</div>
        </Show>
        <Show when={info()}>
          <div class="bce-info">{info()}</div>
        </Show>
      </div>

      <div class="bce-footer">
        <button class="bce-btn" onClick={reset} disabled={busy()}>
          Reset
        </button>
        <button class="bce-btn bce-btn-primary" onClick={save} disabled={busy()}>
          {busy() ? "Saving..." : "Save & reload"}
        </button>
      </div>
    </div>
  );
};

export default BookConfigEditor;
