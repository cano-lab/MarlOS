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
  { value: "letter", label: "8.5 × 11 in (US Letter — resume/document)" },
  { value: "a4", label: "210 × 297 mm (A4 — resume/document)" },
];

// Body fonts the renderer has @font-face entries for. The "custom"
// sentinel switches the picker to a free-text input so the author can
// reference any other family they have installed system-wide.
const BODY_FONT_OPTIONS = [
  { value: "EB Garamond", label: "EB Garamond — print serif (default)" },
  {
    value: "Atkinson Hyperlegible",
    label: "Atkinson Hyperlegible — Braille Institute, low-vision",
  },
  { value: "OpenDyslexic", label: "OpenDyslexic — weighted, dyslexia-friendly" },
  { value: "Lexend", label: "Lexend — reading-proficiency sans" },
];
const BODY_FONT_VALUES = new Set(BODY_FONT_OPTIONS.map((o) => o.value));

const TRIM_PRESET_VALUES = new Set(TRIM_PRESETS.map((p) => p.value));
const MM_PER_IN = 25.4;

function parseTrim(size: string): { w: number; h: number } | null {
  const m = /^(\d+(?:\.\d+)?)x(\d+(?:\.\d+)?)$/.exec(size);
  if (!m) return null;
  const w = parseFloat(m[1]);
  const h = parseFloat(m[2]);
  if (!Number.isFinite(w) || !Number.isFinite(h) || w <= 0 || h <= 0) return null;
  return { w, h };
}

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
  // True when the trim is set to anything other than a named preset.
  // Drives the custom-trim panel visibility.
  const [customMode, setCustomMode] = createSignal(
    !TRIM_PRESET_VALUES.has(props.config.trim.size),
  );
  // Unit used by the custom-trim inputs. Stored in book.toml is
  // always inches; the unit toggle only affects what's displayed.
  const [customUnit, setCustomUnit] = createSignal<"in" | "mm">("in");

  createEffect(() => {
    setDraft(structuredClone(props.config));
    setCustomMode(!TRIM_PRESET_VALUES.has(props.config.trim.size));
    setInfo(null);
    setError(null);
  });

  // Custom-trim helpers ----------------------------------------------
  /** Current trim width/height parsed from draft.trim.size, expressed in
   *  inches. Falls back to 6×9 on malformed strings. */
  const trimInches = () => parseTrim(draft().trim.size) ?? { w: 6, h: 9 };
  /** Convert inches → display value for the active unit, rounded to a
   *  sensible precision (3 decimals for in, 1 for mm). */
  const toDisplay = (inches: number) =>
    customUnit() === "mm"
      ? Math.round(inches * MM_PER_IN * 10) / 10
      : Math.round(inches * 1000) / 1000;
  /** Convert a display value back to inches. */
  const toInches = (v: number) => (customUnit() === "mm" ? v / MM_PER_IN : v);

  const setCustomTrim = (wIn: number, hIn: number) => {
    if (!Number.isFinite(wIn) || !Number.isFinite(hIn) || wIn <= 0 || hIn <= 0) return;
    // Cap to 3 decimal places so we don't end up with size strings
    // like "6.0000000001x9.0" from floating-point round-trips.
    const w = Math.round(wIn * 1000) / 1000;
    const h = Math.round(hIn * 1000) / 1000;
    update((d) => (d.trim.size = `${w}x${h}`));
  };

  const onTrimSelectChange = (v: string) => {
    if (v === "custom") {
      setCustomMode(true);
      // If we were on a preset, seed the custom inputs with the same
      // dimensions so the user has a starting point to tweak.
    } else {
      setCustomMode(false);
      update((d) => (d.trim.size = v));
    }
  };

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
          <p class="bce-help" style="margin-top: 12px">
            Front matter — these populate the generated title and
            copyright pages. Leave empty to omit a page.
          </p>
          <label class="bce-field">
            <span>Publisher</span>
            <input
              type="text"
              value={draft().book.publisher ?? ""}
              onInput={(e) =>
                update((d) => (d.book.publisher = e.currentTarget.value))
              }
            />
          </label>
          <div class="bce-row">
            <label class="bce-field bce-field-narrow">
              <span>Copyright year</span>
              <input
                type="text"
                placeholder="(current year)"
                value={draft().book.copyright_year ?? ""}
                onInput={(e) =>
                  update((d) => (d.book.copyright_year = e.currentTarget.value))
                }
              />
            </label>
            <label class="bce-field bce-field-narrow">
              <span>Copyright holder</span>
              <input
                type="text"
                placeholder="(falls back to author)"
                value={draft().book.copyright_holder ?? ""}
                onInput={(e) =>
                  update((d) => (d.book.copyright_holder = e.currentTarget.value))
                }
              />
            </label>
          </div>
          <label class="bce-field">
            <span>Dedication</span>
            <input
              type="text"
              placeholder='e.g. "For my parents."'
              value={draft().book.dedication ?? ""}
              onInput={(e) =>
                update((d) => (d.book.dedication = e.currentTarget.value))
              }
            />
          </label>
          <label class="bce-field">
            <span>Acknowledgements</span>
            <textarea
              class="bce-textarea"
              placeholder={
                "Centered italic block printed at the end of the book.\n" +
                "Separate paragraphs with a blank line."
              }
              value={draft().book.acknowledgements ?? ""}
              onInput={(e) =>
                update((d) => (d.book.acknowledgements = e.currentTarget.value))
              }
              rows={6}
            />
          </label>
          <label class="bce-field">
            <span>Cover art credit</span>
            <textarea
              class="bce-textarea"
              placeholder={
                'e.g. "Untitled (oil on canvas) by Jane Doe"\n' +
                "Prints anchored top-left on the copyright page."
              }
              value={draft().book.cover_art ?? ""}
              onInput={(e) =>
                update((d) => (d.book.cover_art = e.currentTarget.value))
              }
              rows={3}
            />
          </label>
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
            <select
              value={
                BODY_FONT_VALUES.has(draft().typography.body_font)
                  ? draft().typography.body_font
                  : "__custom__"
              }
              onChange={(e) => {
                const v = e.currentTarget.value;
                if (v === "__custom__") {
                  // Switch to custom; clear to an empty string so the
                  // free-text input takes over (user types their face).
                  update((d) => (d.typography.body_font = ""));
                } else {
                  update((d) => (d.typography.body_font = v));
                }
              }}
            >
              {BODY_FONT_OPTIONS.map((o) => (
                <option value={o.value}>{o.label}</option>
              ))}
              <option value="__custom__">Custom — type a font name</option>
            </select>
          </label>
          <Show when={!BODY_FONT_VALUES.has(draft().typography.body_font)}>
            <label class="bce-field">
              <span>Custom font name</span>
              <input
                type="text"
                value={draft().typography.body_font}
                placeholder="e.g. Garamond, Sabon, Iowan Old Style"
                onInput={(e) =>
                  update(
                    (d) => (d.typography.body_font = e.currentTarget.value)
                  )
                }
              />
            </label>
            <p class="bce-help">
              Only the bundled fonts (EB Garamond, Atkinson Hyperlegible,
              OpenDyslexic, Lexend) ship embedded — a custom face must be
              installed on the printing machine, otherwise the PDF falls
              back to the next family in the stack and KDP may reject it.
            </p>
          </Show>
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
          <label class="bce-field">
            <span>Running header</span>
            <select
              value={draft().typography.running_header_style ?? "title"}
              onChange={(e) =>
                update(
                  (d) =>
                    (d.typography.running_header_style =
                      e.currentTarget.value),
                )
              }
            >
              <option value="title">Title only — "The Equation…"</option>
              <option value="chapter-number">"Chapter 3" — number only</option>
              <option value="chapter-number-title">"Chapter 3 · Title"</option>
              <option value="compact-arabic">"3 · Title" — compact arabic</option>
              <option value="compact-roman">"III · Title" — compact roman</option>
            </select>
          </label>
          <p class="bce-help">
            Strip at the top of each page within a chapter. Helps the
            reader locate themselves without remembering the title.
          </p>
          <label class="bce-field">
            <span>Chapter opener</span>
            <select
              value={draft().typography.chapter_opener_style ?? "modern"}
              onChange={(e) =>
                update(
                  (d) =>
                    (d.typography.chapter_opener_style =
                      e.currentTarget.value),
                )
              }
            >
              <option value="modern">
                Modern — centered small-caps title, simple drop cap
              </option>
              <option value="traditional">
                Traditional — large 4-line drop cap, small-caps lead-in
              </option>
            </select>
          </label>
          <p class="bce-help">
            Visual style of the first page of every chapter. Modern is
            the V1 default; traditional is the novel-style large
            initial cap with the first few words in small caps.
          </p>
          <label class="bce-field">
            <span>Page numbers</span>
            <select
              value={draft().typography.page_number_style ?? "arabic"}
              onChange={(e) =>
                update(
                  (d) =>
                    (d.typography.page_number_style =
                      e.currentTarget.value),
                )
              }
            >
              <option value="arabic">Arabic — 1, 2, 3</option>
              <option value="roman">Roman — I, II, III</option>
              <option value="lower-roman">Lower roman — i, ii, iii</option>
              <option value="none">None — hide page numbers</option>
            </select>
          </label>
          <p class="bce-help">
            Applies to body pages (chapters, interludes, back matter).
          </p>
          <label class="bce-field">
            <span>Front-matter page numbers</span>
            <select
              value={
                draft().typography.front_matter_page_number_style ?? "lower-roman"
              }
              onChange={(e) =>
                update(
                  (d) =>
                    (d.typography.front_matter_page_number_style =
                      e.currentTarget.value),
                )
              }
            >
              <option value="lower-roman">Lower roman — i, ii, iii</option>
              <option value="upper-roman">Upper roman — I, II, III</option>
              <option value="arabic">
                Arabic everywhere — one continuous sequence
              </option>
            </select>
          </label>
          <p class="bce-help">
            Numbering for front matter (Author's Note, Glossary, etc.).
            "Arabic everywhere" makes the whole book one continuous arabic
            sequence — the body does not restart at 1 at chapter 1.
          </p>
        </section>

        <section class="bce-section">
          <h4>Back matter</h4>
          <label class="bce-check">
            <input
              type="checkbox"
              checked={draft().export.include_list_of_figures ?? true}
              onChange={(e) =>
                update(
                  (d) =>
                    (d.export.include_list_of_figures = e.currentTarget.checked),
                )
              }
            />
            <span>Include “List of Figures” &amp; number figure captions</span>
          </label>
          <p class="bce-help">
            Adds a “List of Figures” page at the back and prefixes each
            captioned figure with “Fig. N.”. Turn off for books that
            shouldn’t enumerate their figures.
          </p>
        </section>

        <section class="bce-section">
          <h4>Accessibility</h4>
          <label class="bce-check">
            <input
              type="checkbox"
              checked={draft().export.word_anchors ?? false}
              onChange={(e) =>
                update(
                  (d) => (d.export.word_anchors = e.currentTarget.checked),
                )
              }
            />
            <span>Word anchors — bold the leading half of each word</span>
          </label>
          <p class="bce-help">
            Marks a fixation point at the start of each prose word so
            the eye lands faster. Applies only to chapters &amp;
            interludes — math anchors, equations, code, headings, and
            front/back matter are left as-is. Orthogonal to the
            body-font choice; pair with Atkinson Hyperlegible or
            OpenDyslexic for the strongest low-vision / dyslexia
            support.
          </p>
        </section>

        <section class="bce-section">
          <h4>Trim & margins</h4>
          <label class="bce-field">
            <span>Trim size</span>
            <select
              value={customMode() ? "custom" : draft().trim.size}
              onChange={(e) => onTrimSelectChange(e.currentTarget.value)}
            >
              {TRIM_PRESETS.map((p) => (
                <option value={p.value}>{p.label}</option>
              ))}
              <option value="custom">Custom…</option>
            </select>
          </label>
          <Show when={customMode()}>
            <div class="bce-row">
              <label class="bce-field bce-field-narrow">
                <span>Width</span>
                <input
                  type="number"
                  step={customUnit() === "in" ? "0.01" : "0.1"}
                  value={toDisplay(trimInches().w)}
                  onInput={(e) => {
                    const v = parseFloat(e.currentTarget.value);
                    if (Number.isFinite(v) && v > 0) {
                      setCustomTrim(toInches(v), trimInches().h);
                    }
                  }}
                />
              </label>
              <label class="bce-field bce-field-narrow">
                <span>Height</span>
                <input
                  type="number"
                  step={customUnit() === "in" ? "0.01" : "0.1"}
                  value={toDisplay(trimInches().h)}
                  onInput={(e) => {
                    const v = parseFloat(e.currentTarget.value);
                    if (Number.isFinite(v) && v > 0) {
                      setCustomTrim(trimInches().w, toInches(v));
                    }
                  }}
                />
              </label>
              <label class="bce-field bce-field-narrow">
                <span>Unit</span>
                <select
                  value={customUnit()}
                  onChange={(e) =>
                    setCustomUnit(e.currentTarget.value as "in" | "mm")
                  }
                >
                  <option value="in">inches</option>
                  <option value="mm">mm</option>
                </select>
              </label>
            </div>
            <p class="bce-help">
              Stored in book.toml as inches regardless of the unit you
              type. Common metric trims: 148 × 210 mm (A5), 129 × 198 mm
              (B-format), 152 × 229 mm (Royal).
            </p>
          </Show>
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
