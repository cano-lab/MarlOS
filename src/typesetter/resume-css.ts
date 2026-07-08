/**
 * On-screen (Paged.js) resume/custom stylesheet — the flat single-flow
 * lane's preview CSS. Mirrors the Rust `build_resume_css`
 * (src-tauri/src/typesetter/resume_export.rs) so the preview matches the
 * exported PDF. Kept separate from book-css.ts so the book and resume
 * layout concerns stay in their own lanes.
 *
 * Fonts are referenced by family only — the Book Mode component imports the
 * @fontsource faces, so no @font-face block is needed for the preview (the
 * PDF path embeds them itself). The user's custom.css is appended last so
 * the Kimi-powered Style panel's generated rules override.
 */

import type { BookConfig } from "../services/typesetter-service";
import { trimDimensions } from "./book-css";

/** Trim a float to a compact string (2.0 -> "2", 0.75 -> "0.75"). */
function fmt(v: number): string {
  return `${Math.round(v * 1000) / 1000}`;
}

export function buildResumeCss(config: BookConfig, customCss?: string): string {
  const trim = trimDimensions(config.trim.size);
  const m = config.trim.margins_in;
  const bodyFontName = config.typography.body_font?.trim() || "EB Garamond";
  const bodyFont = `"${bodyFontName}", Georgia, "Times New Roman", serif`;
  const size = `${config.typography.body_size_pt}pt`;
  const lead = `${config.typography.body_leading_pt}pt`;

  return `
@page {
  size: ${trim.width} ${trim.height};
  margin: ${fmt(m.top)}in ${fmt(m.outside)}in ${fmt(m.bottom)}in ${fmt(m.inside)}in;
}

*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; padding: 0; }

body {
  font-family: ${bodyFont};
  font-size: ${size};
  line-height: ${lead};
  color: #111111;
}

main.resume { display: block; }

h1 { font-size: 1.9em; line-height: 1.1; margin: 0 0 0.1em; }
h2 {
  font-size: 1.15em;
  margin: 1.1em 0 0.35em;
  padding-bottom: 0.1em;
  border-bottom: 0.75pt solid #999999;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
h3 { font-size: 1em; margin: 0.7em 0 0.15em; }
h1, h2, h3, h4 { break-after: avoid; }

p { margin: 0.35em 0; }
ul, ol { margin: 0.3em 0 0.35em 1.2em; padding: 0; }
li { margin: 0.15em 0; }
a { color: inherit; text-decoration: none; }
strong { font-weight: 700; }
em { font-style: italic; }
hr { border: none; border-top: 0.75pt solid #999999; margin: 0.7em 0; }

table { width: 100%; border-collapse: collapse; }
td, th { text-align: left; vertical-align: top; padding: 0.1em 0.4em 0.1em 0; }
${customCss ? `\n/* custom.css — overrides the base resume styling. */\n${customCss}\n` : ""}`;
}
