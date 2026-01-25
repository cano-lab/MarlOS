import { Component, createSignal, createEffect, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import "./PrintPreview.css";

interface PrintPreviewProps {
  type: "markdown" | "pdf";
  content?: string; // For markdown
  pdfPath?: string; // For PDF
  onClose: () => void;
}

interface PagePreview {
  index: number;
  imageData: string;
  width: number;
  height: number;
}

const PrintPreview: Component<PrintPreviewProps> = (props) => {
  const [pages, setPages] = createSignal<PagePreview[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [selectedPages, setSelectedPages] = createSignal<Set<number>>(new Set());
  const [paperSize, setPaperSize] = createSignal<"letter" | "a4" | "legal">("letter");
  const [orientation, setOrientation] = createSignal<"portrait" | "landscape">("portrait");
  const [scale, setScale] = createSignal(100);
  const [currentPreviewPage, setCurrentPreviewPage] = createSignal(0);

  // Margins in inches
  const [marginTop, setMarginTop] = createSignal(1.0);
  const [marginRight, setMarginRight] = createSignal(1.0);
  const [marginBottom, setMarginBottom] = createSignal(1.0);
  const [marginLeft, setMarginLeft] = createSignal(1.0);

  // Paper dimensions in points (72 points = 1 inch)
  const paperDimensions = {
    letter: { width: 612, height: 792 }, // 8.5 x 11 inches
    a4: { width: 595, height: 842 }, // 210 x 297 mm
    legal: { width: 612, height: 1008 }, // 8.5 x 14 inches
  };

  createEffect(async () => {
    setLoading(true);
    setError(null);

    try {
      if (props.type === "pdf" && props.pdfPath) {
        await loadPdfPages();
      } else if (props.type === "markdown" && props.content) {
        await loadMarkdownPreview();
      }
    } catch (e) {
      setError(`Failed to generate preview: ${e}`);
    } finally {
      setLoading(false);
    }
  });

  const loadPdfPages = async () => {
    try {
      // Get PDF info
      const info = await invoke<{ page_count: number }>("pdf_get_info");
      const pageCount = info.page_count;

      // Render each page at 150 DPI for preview
      const previewPages: PagePreview[] = [];
      for (let i = 0; i < pageCount; i++) {
        const rendered = await invoke<{
          image_data: string;
          width_px: number;
          height_px: number;
        }>("pdf_render_page", { pageIndex: i, dpi: 150 });

        previewPages.push({
          index: i,
          imageData: rendered.image_data,
          width: rendered.width_px,
          height: rendered.height_px,
        });
      }

      setPages(previewPages);
      // Select all pages by default
      setSelectedPages(new Set(previewPages.map(p => p.index)));
    } catch (e) {
      throw new Error(`PDF preview failed: ${e}`);
    }
  };

  const loadMarkdownPreview = async () => {
    // For markdown, we'll create a single page preview from the rendered HTML
    // The actual printing will use the browser's print functionality
    setPages([{
      index: 0,
      imageData: "", // We'll render HTML instead
      width: paperDimensions[paperSize()].width,
      height: paperDimensions[paperSize()].height,
    }]);
    setSelectedPages(new Set([0]));
  };

  const togglePage = (index: number) => {
    const current = new Set(selectedPages());
    if (current.has(index)) {
      current.delete(index);
    } else {
      current.add(index);
    }
    setSelectedPages(current);
  };

  const selectAllPages = () => {
    setSelectedPages(new Set(pages().map(p => p.index)));
  };

  const deselectAllPages = () => {
    setSelectedPages(new Set());
  };

  const handlePrint = () => {
    if (props.type === "markdown") {
      printMarkdown();
    } else {
      printPdf();
    }
  };

  const printMarkdown = () => {
    // Create a hidden iframe for printing
    const printFrame = document.createElement("iframe");
    printFrame.style.position = "fixed";
    printFrame.style.right = "0";
    printFrame.style.bottom = "0";
    printFrame.style.width = "0";
    printFrame.style.height = "0";
    printFrame.style.border = "0";
    document.body.appendChild(printFrame);

    const doc = printFrame.contentDocument || printFrame.contentWindow?.document;
    if (!doc) return;

    // Get the rendered preview content
    const previewContent = document.querySelector(".print-preview-markdown-content");
    const htmlContent = previewContent?.innerHTML || "";

    doc.open();
    doc.write(`
      <!DOCTYPE html>
      <html>
      <head>
        <title>Print Preview</title>
        <style>
          @page {
            size: ${paperSize()} ${orientation()};
            margin: ${marginTop()}in ${marginRight()}in ${marginBottom()}in ${marginLeft()}in;
          }
          body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 100%;
          }
          h1, h2, h3, h4, h5, h6 {
            margin-top: 1.5em;
            margin-bottom: 0.5em;
          }
          h1 { font-size: 2em; }
          h2 { font-size: 1.5em; }
          h3 { font-size: 1.25em; }
          pre {
            background: #f5f5f5;
            padding: 1em;
            border-radius: 4px;
            overflow-x: auto;
          }
          code {
            font-family: 'Fira Code', Consolas, monospace;
          }
          table {
            border-collapse: collapse;
            width: 100%;
            margin: 1em 0;
          }
          th, td {
            border: 1px solid #ddd;
            padding: 8px;
            text-align: left;
          }
          th {
            background: #f5f5f5;
          }
          img {
            max-width: 100%;
            height: auto;
          }
          blockquote {
            border-left: 4px solid #ddd;
            margin: 1em 0;
            padding-left: 1em;
            color: #666;
          }
        </style>
      </head>
      <body>
        ${htmlContent}
      </body>
      </html>
    `);
    doc.close();

    // Wait for content to load then print
    setTimeout(() => {
      printFrame.contentWindow?.print();
      // Remove iframe after printing
      setTimeout(() => {
        document.body.removeChild(printFrame);
      }, 1000);
    }, 250);

    props.onClose();
  };

  const printPdf = () => {
    // For PDF, create a page with all selected page images
    const selectedPagesList = Array.from(selectedPages()).sort((a, b) => a - b);
    const selectedImages = pages().filter(p => selectedPagesList.includes(p.index));

    const printFrame = document.createElement("iframe");
    printFrame.style.position = "fixed";
    printFrame.style.right = "0";
    printFrame.style.bottom = "0";
    printFrame.style.width = "0";
    printFrame.style.height = "0";
    printFrame.style.border = "0";
    document.body.appendChild(printFrame);

    const doc = printFrame.contentDocument || printFrame.contentWindow?.document;
    if (!doc) return;

    const imagesHtml = selectedImages.map(page => `
      <div class="pdf-page" style="page-break-after: always;">
        <img src="data:image/png;base64,${page.imageData}"
             style="max-width: 100%; height: auto; display: block; margin: 0 auto;" />
      </div>
    `).join("");

    doc.open();
    doc.write(`
      <!DOCTYPE html>
      <html>
      <head>
        <title>Print PDF</title>
        <style>
          @page {
            size: ${paperSize()} ${orientation()};
            margin: ${marginTop()}in ${marginRight()}in ${marginBottom()}in ${marginLeft()}in;
          }
          body {
            margin: 0;
            padding: 0;
          }
          .pdf-page {
            display: flex;
            justify-content: center;
            align-items: flex-start;
          }
          .pdf-page:last-child {
            page-break-after: avoid;
          }
        </style>
      </head>
      <body>
        ${imagesHtml}
      </body>
      </html>
    `);
    doc.close();

    setTimeout(() => {
      printFrame.contentWindow?.print();
      setTimeout(() => {
        document.body.removeChild(printFrame);
      }, 1000);
    }, 500);

    props.onClose();
  };

  const renderMarkdownContent = () => {
    if (!props.content) return "";
    // Use the same marked rendering as Preview component
    return marked(props.content) as string;
  };

  return (
    <div class="print-preview-overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="print-preview-dialog">
        <div class="print-preview-header">
          <h2>Print Preview</h2>
          <button class="close-btn" onClick={props.onClose}>×</button>
        </div>

        <div class="print-preview-body">
          {/* Sidebar with options */}
          <div class="print-options">
            <div class="option-group">
              <label>Paper Size</label>
              <select value={paperSize()} onChange={(e) => setPaperSize(e.target.value as any)}>
                <option value="letter">Letter (8.5" × 11")</option>
                <option value="a4">A4 (210 × 297 mm)</option>
                <option value="legal">Legal (8.5" × 14")</option>
              </select>
            </div>

            <div class="option-group">
              <label>Orientation</label>
              <select value={orientation()} onChange={(e) => setOrientation(e.target.value as any)}>
                <option value="portrait">Portrait</option>
                <option value="landscape">Landscape</option>
              </select>
            </div>

            <div class="option-group">
              <label>Scale: {scale()}%</label>
              <input
                type="range"
                min="50"
                max="200"
                value={scale()}
                onInput={(e) => setScale(parseInt(e.target.value))}
              />
            </div>

            <div class="option-group">
              <label>Margins (inches)</label>
              <div class="margin-grid">
                <div class="margin-row">
                  <span>Top:</span>
                  <input
                    type="number"
                    min="0"
                    max="3"
                    step="0.25"
                    value={marginTop()}
                    onInput={(e) => setMarginTop(parseFloat(e.target.value) || 0)}
                  />
                </div>
                <div class="margin-row">
                  <span>Right:</span>
                  <input
                    type="number"
                    min="0"
                    max="3"
                    step="0.25"
                    value={marginRight()}
                    onInput={(e) => setMarginRight(parseFloat(e.target.value) || 0)}
                  />
                </div>
                <div class="margin-row">
                  <span>Bottom:</span>
                  <input
                    type="number"
                    min="0"
                    max="3"
                    step="0.25"
                    value={marginBottom()}
                    onInput={(e) => setMarginBottom(parseFloat(e.target.value) || 0)}
                  />
                </div>
                <div class="margin-row">
                  <span>Left:</span>
                  <input
                    type="number"
                    min="0"
                    max="3"
                    step="0.25"
                    value={marginLeft()}
                    onInput={(e) => setMarginLeft(parseFloat(e.target.value) || 0)}
                  />
                </div>
              </div>
            </div>

            <Show when={props.type === "pdf" && pages().length > 1}>
              <div class="option-group">
                <label>Pages ({selectedPages().size} of {pages().length})</label>
                <div class="page-selection-actions">
                  <button onClick={selectAllPages}>All</button>
                  <button onClick={deselectAllPages}>None</button>
                </div>
                <div class="page-thumbnails">
                  <For each={pages()}>
                    {(page) => (
                      <div
                        class={`page-thumbnail ${selectedPages().has(page.index) ? "selected" : ""}`}
                        onClick={() => togglePage(page.index)}
                      >
                        <Show when={page.imageData}>
                          <img src={`data:image/png;base64,${page.imageData}`} alt={`Page ${page.index + 1}`} />
                        </Show>
                        <span>{page.index + 1}</span>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>

          {/* Preview area */}
          <div class="print-preview-content">
            <Show when={loading()}>
              <div class="preview-loading">Generating preview...</div>
            </Show>

            <Show when={error()}>
              <div class="preview-error">{error()}</div>
            </Show>

            <Show when={!loading() && !error()}>
              <Show when={props.type === "markdown"}>
                <div
                  class="preview-page"
                  style={{
                    width: `${(paperDimensions[paperSize()].width * scale() / 100)}px`,
                    "min-height": `${(paperDimensions[paperSize()].height * scale() / 100)}px`,
                    "padding-top": `${marginTop() * 72 * scale() / 100}px`,
                    "padding-right": `${marginRight() * 72 * scale() / 100}px`,
                    "padding-bottom": `${marginBottom() * 72 * scale() / 100}px`,
                    "padding-left": `${marginLeft() * 72 * scale() / 100}px`,
                    "box-sizing": "border-box",
                  }}
                >
                  <div
                    class="print-preview-markdown-content"
                    innerHTML={renderMarkdownContent()}
                  />
                </div>
              </Show>

              <Show when={props.type === "pdf"}>
                <div class="pdf-preview-pages">
                  <For each={pages().filter(p => selectedPages().has(p.index))}>
                    {(page) => (
                      <div
                        class="preview-page pdf-page"
                        style={{ transform: `scale(${scale() / 100})` }}
                      >
                        <img
                          src={`data:image/png;base64,${page.imageData}`}
                          alt={`Page ${page.index + 1}`}
                        />
                        <div class="page-number">Page {page.index + 1}</div>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </div>
        </div>

        <div class="print-preview-footer">
          <button class="btn-secondary" onClick={props.onClose}>Cancel</button>
          <button
            class="btn-primary"
            onClick={handlePrint}
            disabled={loading() || selectedPages().size === 0}
          >
            Print
          </button>
        </div>
      </div>
    </div>
  );
};

export default PrintPreview;
