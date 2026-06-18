import { Component, createEffect, createSignal } from "solid-js";
import { marked } from "marked";
import mermaid from "mermaid";
import katex from "katex";
import "katex/dist/katex.min.css";
import "./Preview.css";

// SECURITY: Escape HTML to prevent XSS in error messages
const escapeHtml = (text: string): string => {
  const htmlEscapes: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  return text.replace(/[&<>"']/g, (char) => htmlEscapes[char]);
};

interface PreviewProps {
  content: string;
}

// Initialize mermaid with dark theme
mermaid.initialize({
  startOnLoad: false,
  theme: "dark",
  themeVariables: {
    primaryColor: "#569cd6",
    primaryTextColor: "#d4d4d4",
    primaryBorderColor: "#3c3c3c",
    lineColor: "#6a9955",
    secondaryColor: "#4ec9b0",
    tertiaryColor: "#2d2d2d",
  },
});

// Configure marked with custom renderer for extended markdown
const renderer = {
  code(code: string, language: string | undefined) {
    if (language === "mermaid") {
      return `<div class="mermaid-container" data-mermaid="${encodeURIComponent(code)}"></div>`;
    }
    if (language === "chart") {
      return `<div class="chart-container" data-chart="${encodeURIComponent(code)}"></div>`;
    }
    if (language === "math" || language === "latex") {
      try {
        return `<div class="math-block">${katex.renderToString(code, { displayMode: true })}</div>`;
      } catch (e) {
        return `<div class="math-error">Math Error: ${escapeHtml((e as Error).message)}</div>`;
      }
    }
    // Default code block
    const escaped = code.replace(/</g, "&lt;").replace(/>/g, "&gt;");
    return `<pre class="code-block"><code class="language-${language || 'text'}">${escaped}</code></pre>`;
  },
};

marked.use({ renderer } as any);

// Process inline math ($...$) and display math ($$...$$)
const processMath = (html: string): string => {
  // Display math: $$...$$
  html = html.replace(/\$\$([\s\S]*?)\$\$/g, (_, math) => {
    try {
      return `<div class="math-block">${katex.renderToString(math.trim(), { displayMode: true })}</div>`;
    } catch (e) {
      return `<span class="math-error">Math Error: ${(e as Error).message}</span>`;
    }
  });

  // Inline math: $...$
  html = html.replace(/\$([^\$\n]+?)\$/g, (_, math) => {
    try {
      return katex.renderToString(math.trim(), { displayMode: false });
    } catch (e) {
      return `<span class="math-error">${escapeHtml((e as Error).message)}</span>`;
    }
  });

  return html;
};

const Preview: Component<PreviewProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  const [html, setHtml] = createSignal("");

  const renderMermaidDiagrams = async () => {
    if (!containerRef) return;

    const mermaidContainers = containerRef.querySelectorAll(".mermaid-container");
    for (let i = 0; i < mermaidContainers.length; i++) {
      const container = mermaidContainers[i] as HTMLElement;
      const code = decodeURIComponent(container.dataset.mermaid || "");
      if (code) {
        try {
          const id = `mermaid-${Date.now()}-${i}`;
          const { svg } = await mermaid.render(id, code);
          container.innerHTML = svg;
          container.classList.add("rendered");
        } catch (e) {
          container.innerHTML = `<div class="mermaid-error">Diagram Error: ${escapeHtml((e as Error).message)}</div>`;
          container.classList.add("error");
        }
      }
    }
  };

  const renderCharts = () => {
    if (!containerRef) return;

    const chartContainers = containerRef.querySelectorAll(".chart-container");
    chartContainers.forEach((container, i) => {
      const el = container as HTMLElement;
      const code = decodeURIComponent(el.dataset.chart || "");
      if (code) {
        try {
          // Parse YAML-like chart config
          const config = parseChartConfig(code);
          const canvas = document.createElement("canvas");
          canvas.id = `chart-${Date.now()}-${i}`;
          el.appendChild(canvas);

          // Dynamic import for Chart.js to avoid SSR issues
          import("chart.js/auto").then(({ default: Chart }) => {
            new Chart(canvas, config);
          });
          el.classList.add("rendered");
        } catch (e) {
          el.innerHTML = `<div class="chart-error">Chart Error: ${escapeHtml((e as Error).message)}</div>`;
          el.classList.add("error");
        }
      }
    });
  };

  // Simple YAML-like parser for chart config
  const parseChartConfig = (code: string): any => {
    const lines = code.split("\n");
    let type = "bar";
    const labels: string[] = [];
    const datasets: any[] = [];
    let currentDataset: any = null;

    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.startsWith("type:")) {
        type = trimmed.split(":")[1].trim();
      } else if (trimmed.startsWith("labels:")) {
        const match = trimmed.match(/\[(.*)\]/);
        if (match) {
          labels.push(...match[1].split(",").map(s => s.trim()));
        }
      } else if (trimmed.startsWith("- label:")) {
        if (currentDataset) datasets.push(currentDataset);
        currentDataset = { label: trimmed.split(":")[1].trim() };
      } else if (trimmed.startsWith("data:") && currentDataset) {
        const match = trimmed.match(/\[(.*)\]/);
        if (match) {
          currentDataset.data = match[1].split(",").map(s => parseFloat(s.trim()));
        }
      } else if (trimmed.startsWith("backgroundColor:") && currentDataset) {
        currentDataset.backgroundColor = trimmed.split(":").slice(1).join(":").trim();
      }
    }
    if (currentDataset) datasets.push(currentDataset);

    return {
      type,
      data: { labels, datasets },
      options: {
        responsive: true,
        plugins: {
          legend: { labels: { color: "#d4d4d4" } },
        },
        scales: {
          x: { ticks: { color: "#9d9d9d" }, grid: { color: "#3c3c3c" } },
          y: { ticks: { color: "#9d9d9d" }, grid: { color: "#3c3c3c" } },
        },
      },
    };
  };

  createEffect(() => {
    const content = props.content;
    let rendered = marked.parse(content) as string;
    rendered = processMath(rendered);
    setHtml(rendered);
  });

  createEffect(() => {
    // Re-render diagrams when HTML changes
    void html();
    setTimeout(() => {
      renderMermaidDiagrams();
      renderCharts();
    }, 50);
  });

  return (
    <div class="preview" ref={containerRef}>
      <div class="preview-content" innerHTML={html()} />
    </div>
  );
};

export default Preview;
