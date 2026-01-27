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

  // Smart printing options
  const [smartPrinting, setSmartPrinting] = createSignal(true);
  const [showPageNumbers, setShowPageNumbers] = createSignal(true);

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

    // Smart printing CSS rules
    const smartPrintingStyles = smartPrinting() ? `
          /* Widow/orphan control - prevent single lines at top/bottom of pages */
          p, li {
            orphans: 3;
            widows: 3;
          }

          /* Prevent headings from being orphaned at bottom of page */
          h1, h2, h3, h4, h5, h6 {
            break-after: avoid;
            page-break-after: avoid;
          }

          /* Keep headings with their following content */
          h1 + *, h2 + *, h3 + *, h4 + *, h5 + *, h6 + * {
            break-before: avoid;
            page-break-before: avoid;
          }

          /* Prevent breaking inside code blocks, quotes, and small tables */
          pre, blockquote {
            break-inside: avoid;
            page-break-inside: avoid;
          }

          /* Tables: avoid breaking inside rows, add continuation headers */
          table {
            break-inside: auto;
          }
          thead {
            display: table-header-group;
          }
          tr {
            break-inside: avoid;
            page-break-inside: avoid;
          }

          /* Figures with captions - keep together when possible */
          figure {
            break-inside: avoid;
            page-break-inside: avoid;
          }

          /* For large figures that must break, show continuation notice */
          .figure-container {
            position: relative;
          }
          .figure-container[data-title]::before {
            content: attr(data-title);
            display: none;
          }

          /* Images - try to keep with captions */
          img {
            break-inside: avoid;
            page-break-inside: avoid;
          }

          /* Keep list items together when reasonable */
          li {
            break-inside: avoid;
            page-break-inside: avoid;
          }

          /* Prevent large gaps before headers */
          h1, h2, h3 {
            margin-top: 1em;
          }
    ` : '';

    // Page numbering styles
    const pageNumberStyles = showPageNumbers() ? `
          @page {
            @bottom-center {
              content: counter(page);
            }
          }
          body {
            counter-reset: page;
          }
    ` : '';

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
            font-size: 0.85em;
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
          figure {
            margin: 1.5em 0;
            text-align: center;
          }
          figcaption {
            font-size: 0.9em;
            color: #666;
            margin-top: 0.5em;
            font-style: italic;
          }
          ${smartPrintingStyles}
          ${pageNumberStyles}
        </style>
      </head>
      <body>
        ${processContentForPrint(htmlContent)}
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

  // Process HTML content for smart printing (add data attributes for continuation headers)
  const processContentForPrint = (html: string): string => {
    if (!smartPrinting()) return html;

    // Create a temporary DOM to process the HTML
    const temp = document.createElement('div');
    temp.innerHTML = html;

    // Wrap images with figures and add data-title for continuation
    const images = temp.querySelectorAll('img');
    images.forEach((img, index) => {
      const alt = img.getAttribute('alt') || `Figure ${index + 1}`;

      // Check if already wrapped in figure
      if (img.parentElement?.tagName !== 'FIGURE') {
        const figure = document.createElement('figure');
        figure.className = 'figure-container';
        figure.setAttribute('data-title', `${alt} (continued)`);

        const figcaption = document.createElement('figcaption');
        figcaption.textContent = alt;

        img.parentNode?.insertBefore(figure, img);
        figure.appendChild(img);
        figure.appendChild(figcaption);
      } else {
        // Add data-title to existing figure
        const figure = img.parentElement as HTMLElement;
        const caption = figure.querySelector('figcaption');
        const title = caption?.textContent || alt;
        figure.setAttribute('data-title', `${title} (continued)`);
      }
    });

    // Process tables - ensure they have thead for repeat headers
    const tables = temp.querySelectorAll('table');
    tables.forEach((table, index) => {
      // Find the caption or create one
      let caption = table.querySelector('caption');
      if (!caption) {
        // Look for preceding heading or paragraph as title
        const prevSibling = table.previousElementSibling;
        if (prevSibling && (prevSibling.tagName === 'P' || prevSibling.tagName.match(/^H[1-6]$/))) {
          const text = prevSibling.textContent?.trim();
          if (text && text.length < 100) {
            caption = document.createElement('caption');
            caption.textContent = text;
            table.insertBefore(caption, table.firstChild);
          }
        }
      }

      // Ensure first row is in thead if it contains th elements
      const firstRow = table.querySelector('tr');
      const hasThInFirstRow = firstRow?.querySelector('th');

      if (hasThInFirstRow && !table.querySelector('thead')) {
        const thead = document.createElement('thead');
        const tbody = table.querySelector('tbody') || document.createElement('tbody');

        // Move first row to thead
        if (firstRow) {
          thead.appendChild(firstRow);
        }

        // Wrap remaining rows in tbody if not already
        const remainingRows = table.querySelectorAll('tr');
        if (!table.querySelector('tbody')) {
          remainingRows.forEach(row => tbody.appendChild(row));
        }

        // Insert thead at beginning
        table.insertBefore(thead, table.firstChild);
        if (!table.querySelector('tbody')) {
          table.appendChild(tbody);
        }
      }
    });

    return temp.innerHTML;
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

            <Show when={props.type === "markdown"}>
              <div class="option-group">
                <label>Smart Printing</label>
                <div class="checkbox-row">
                  <input
                    type="checkbox"
                    id="smartPrinting"
                    checked={smartPrinting()}
                    onChange={(e) => setSmartPrinting(e.target.checked)}
                  />
                  <label for="smartPrinting">Enable smart layout</label>
                </div>
                <div class="smart-print-details">
                  <ul>
                    <li>No orphaned headings at page bottom</li>
                    <li>Table headers repeat on each page</li>
                    <li>Figures keep captions together</li>
                    <li>Code blocks avoid mid-break</li>
                  </ul>
                </div>
              </div>

              <div class="option-group">
                <label>Page Numbers</label>
                <div class="checkbox-row">
                  <input
                    type="checkbox"
                    id="showPageNumbers"
                    checked={showPageNumbers()}
                    onChange={(e) => setShowPageNumbers(e.target.checked)}
                  />
                  <label for="showPageNumbers">Show page numbers</label>
                </div>
              </div>
            </Show>

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
