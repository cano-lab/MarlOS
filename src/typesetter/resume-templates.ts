/**
 * Starter CSS templates for the flat resume/custom lane. Applying one seeds
 * custom.css with a solid, print-ready base so the AI (and the user) refine
 * from a real layout instead of generating from scratch — the chosen
 * template becomes the `currentCss` every Style-chat turn builds on.
 *
 * Constraints (match the resume system prompt): style + layout ONLY, no
 * `font-family` (the embedded print font stays), pt/in/em units, and only
 * the flat selector vocabulary (main.resume, h1–h4, p, ul/ol/li, hr, table).
 * These flow on arbitrary flat markdown — no assumption about section
 * grouping — so they render reliably on any resume.md.
 */

export interface ResumeTemplate {
  id: string;
  name: string;
  description: string;
  css: string;
}

export const RESUME_TEMPLATES: ResumeTemplate[] = [
  {
    id: "classic",
    name: "Classic",
    description: "Centered name, uppercase section headings with a rule. Traditional single column.",
    css: `@page { margin: 0.7in; }

main.resume { display: block; }

main.resume h1 {
  text-align: center;
  font-size: 22pt;
  font-weight: 700;
  letter-spacing: 0.02em;
  margin: 0 0 0.04in;
}
/* Contact line = the first paragraph right after the name. */
main.resume h1 + p {
  text-align: center;
  font-size: 9.5pt;
  color: #444;
  margin: 0 0 0.2in;
}
main.resume h2 {
  font-size: 11pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  border-bottom: 1pt solid #333;
  padding-bottom: 0.03in;
  margin: 0.22in 0 0.1in;
}
main.resume h3 { font-size: 10pt; font-weight: 700; margin: 0.12in 0 0.02in; }
main.resume h4 { font-size: 9.5pt; font-weight: 600; font-style: italic; color: #555; margin: 0 0 0.04in; }
main.resume p { font-size: 9.5pt; line-height: 1.35; margin: 0.03in 0; }
main.resume ul { margin: 0.04in 0 0.08in 0.95em; padding: 0; }
main.resume li { font-size: 9.5pt; line-height: 1.3; margin: 0.02in 0; }
`,
  },
  {
    id: "modern",
    name: "Modern",
    description: "Left-aligned, accent-coloured headings, generous whitespace.",
    css: `@page { margin: 0.75in; }

main.resume { display: block; color: #1a1a1a; }

main.resume h1 {
  font-size: 26pt;
  font-weight: 700;
  letter-spacing: -0.01em;
  margin: 0 0 0.02in;
}
main.resume h1 + p {
  font-size: 9.5pt;
  color: #2a6fb0;
  margin: 0 0 0.28in;
}
main.resume h2 {
  font-size: 10.5pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: #2a6fb0;
  margin: 0.28in 0 0.1in;
}
main.resume h3 { font-size: 10.5pt; font-weight: 700; margin: 0.14in 0 0.01in; }
main.resume h4 { font-size: 9.5pt; font-weight: 400; font-style: italic; color: #666; margin: 0 0 0.05in; }
main.resume p { font-size: 9.5pt; line-height: 1.45; margin: 0.04in 0; }
main.resume ul { margin: 0.05in 0 0.1in 1em; padding: 0; }
main.resume li { font-size: 9.5pt; line-height: 1.4; margin: 0.03in 0; }
main.resume a { color: #2a6fb0; }
`,
  },
  {
    id: "compact",
    name: "Compact",
    description: "Tight spacing and smaller type to fit more on one page.",
    css: `@page { margin: 0.5in; }

main.resume { display: block; }

main.resume h1 { font-size: 18pt; font-weight: 700; margin: 0 0 0.02in; }
main.resume h1 + p { font-size: 8.5pt; color: #444; margin: 0 0 0.12in; }
main.resume h2 {
  font-size: 9.5pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  border-bottom: 0.5pt solid #aaa;
  padding-bottom: 0.02in;
  margin: 0.12in 0 0.05in;
}
main.resume h3 { font-size: 9pt; font-weight: 700; margin: 0.06in 0 0.01in; }
main.resume h4 { font-size: 8.5pt; font-style: italic; color: #555; margin: 0; }
main.resume p { font-size: 8.5pt; line-height: 1.25; margin: 0.015in 0; }
main.resume ul { margin: 0.02in 0 0.04in 0.85em; padding: 0; }
main.resume li { font-size: 8.5pt; line-height: 1.2; margin: 0.01in 0; }
`,
  },
  {
    id: "two-column",
    name: "Two-column",
    description: "Name spans the top; body flows into two balanced columns.",
    css: `@page { margin: 0.55in; }

main.resume {
  column-count: 2;
  column-gap: 0.4in;
  column-fill: auto;
}

/* Name + contact span the full width above the columns. */
main.resume h1 { column-span: all; font-size: 22pt; font-weight: 700; margin: 0 0 0.03in; }
main.resume h1 + p { column-span: all; font-size: 9pt; color: #444; margin: 0 0 0.18in; }

main.resume h2 {
  font-size: 10pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  border-bottom: 0.75pt solid #999;
  padding-bottom: 0.02in;
  margin: 0.16in 0 0.07in;
  break-after: avoid;
}
main.resume h3 { font-size: 9.5pt; font-weight: 700; margin: 0.09in 0 0.01in; break-after: avoid; }
main.resume h4 { font-size: 9pt; font-style: italic; color: #555; margin: 0 0 0.03in; }
main.resume p { font-size: 9pt; line-height: 1.3; margin: 0.025in 0; }
main.resume ul { margin: 0.03in 0 0.06in; padding-left: 0.22in; list-style-position: outside; }
main.resume li { font-size: 9pt; line-height: 1.25; margin: 0.015in 0; break-inside: avoid; }
`,
  },
];
