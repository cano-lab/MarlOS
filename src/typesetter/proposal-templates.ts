/**
 * Starter templates for research and grant proposals on the flat document
 * lane. Each template provides a CSS seed and a Markdown scaffold so the
 * AI can expand it into full prose when the user provides a topic.
 *
 * Constraints (match the flat-lane system prompt): style + layout ONLY,
 * no `font-family` (the embedded print font stays), pt/in/em units, and
 * only the flat selector vocabulary (main.resume, h1–h4, p, ul/ol/li, hr,
 * table, .abstract, .timeline, .budget). These flow on arbitrary flat
 * markdown, so they render reliably on any proposal.md.
 */

export interface ProposalTemplate {
  id: string;
  name: string;
  description: string;
  /** "funder" = agency-specific structure; "layout" = generic visual lane. */
  category: "funder" | "layout";
  /** Starter custom.css for this proposal. */
  css: string;
  /** Markdown scaffold used when AI expansion is requested. */
  scaffold: string;
  /** Preferred page size identifier. */
  page_size: string;
}

const NIH_R01_SCAFFOLD = `# Project Title

**Principal Investigator:** Your Name
**Institution:** Your Institution
**Funding Agency:** NIH / NIGMS

## Abstract

(250 words summarizing the problem, aims, and expected impact.)

## Specific Aims

1. Aim 1: ...
2. Aim 2: ...
3. Aim 3: ...

## Significance

(Why does this work matter?)

## Innovation

(What is new or transformative?)

## Approach

### Aim 1

(Methods, analysis, and expected outcomes.)

### Aim 2

(Methods, analysis, and expected outcomes.)

### Aim 3

(Methods, analysis, and expected outcomes.)

## Timeline

| Phase | Activity | Months |
|-------|----------|--------|
| 1     | ...      | 1–6    |
| 2     | ...      | 7–18   |
| 3     | ...      | 19–36  |

## Budget Justification

(Brief justification for major cost categories.)

## References

1. ...
`;

const NSF_GRFP_SCAFFOLD = `# Project Title

**Applicant:** Your Name
**Field of Study:** ...

## Graduate Research Plan Statement

### Introduction / Motivation

(The problem you will address.)

### Research Questions

1. ...
2. ...

### Methodology

(How you will answer the questions.)

### Broader Impacts

(How the work benefits society.)

### Timeline

| Year | Milestone |
|------|-----------|
| 1    | ...       |
| 2    | ...       |
| 3    | ...       |

## Personal Statement

(Your preparation and goals.)

## References

1. ...
`;

const ERC_STARTING_SCAFFOLD = `# Project Title

**Principal Investigator:** Your Name
**Host Institution:** ...

## Summary

(Short abstract for a general scientific audience.)

## State-of-the-Art and Objectives

(Research context and ambitious objectives.)

## Methodology

(Innovative approach and work packages.)

## Expected Impact

(Scientific, technological, and societal outcomes.)

## Implementation

### Work Package 1

(Activities and deliverables.)

### Work Package 2

(Activities and deliverables.)

### Work Package 3

(Activities and deliverables.)

## Risk Management and Ethics

(Anticipated risks and mitigation.)

## Budget Overview

| Category | Amount (€) |
|----------|------------|
| Personnel| ...        |
| Equipment| ...        |
| Travel   | ...        |

## References

1. ...
`;

const ACADEMIC_PROPOSAL_SCAFFOLD = `# Project Title

**Author:** Your Name
**Affiliation:** ...

## Abstract

(Brief overview of the proposed work.)

## Introduction

(Background and motivation.)

## Research Objectives

1. ...
2. ...
3. ...

## Methodology

(Approach, methods, and analysis plan.)

## Expected Outcomes

(Results and contributions.)

## Timeline

| Phase | Duration | Activities |
|-------|----------|------------|
| 1     | Months 1–6 | ... |
| 2     | Months 7–12 | ... |

## Budget Summary

| Item | Cost |
|------|------|
| ...  | ...  |

## References

1. ...
`;

const CORPORATE_GRANT_SCAFFOLD = `# Proposal Title

**Submitted to:** Funder / Program
**Date:** ...
**Submitted by:** Organization

## Executive Summary

(One-paragraph overview of the request.)

## Problem Statement

(The need or opportunity.)

## Proposed Solution

(What you will do and why it works.)

## Goals and Deliverables

1. ...
2. ...
3. ...

## Implementation Plan

(Activities, timeline, and responsible parties.)

## Budget

| Category | Amount |
|----------|--------|
| Personnel| ...    |
| Services | ...    |
| Materials| ...    |

## Evaluation

(How success will be measured.)

## Sustainability

(Long-term impact beyond the grant period.)

## References

1. ...
`;

export const PROPOSAL_TEMPLATES: ProposalTemplate[] = [
  {
    id: "nih-r01",
    name: "NIH R01",
    description: "NIH-style research proposal with Specific Aims, Significance, Innovation, and Approach.",
    category: "funder",
    page_size: "letter",
    scaffold: NIH_R01_SCAFFOLD,
    css: `@page { margin: 0.7in; }

main.resume { display: block; }

main.resume h1 {
  text-align: center;
  font-size: 18pt;
  font-weight: 700;
  margin: 0 0 0.15in;
}

/* PI / metadata line right after the title. */
main.resume h1 + p {
  text-align: center;
  font-size: 9.5pt;
  color: #444;
  margin: 0 0 0.25in;
}

main.resume h2 {
  font-size: 11pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  border-bottom: 0.75pt solid #333;
  padding-bottom: 0.03in;
  margin: 0.25in 0 0.1in;
}

main.resume h3 {
  font-size: 10.5pt;
  font-weight: 700;
  margin: 0.14in 0 0.04in;
}

main.resume h4 {
  font-size: 10pt;
  font-weight: 600;
  font-style: italic;
  color: #555;
  margin: 0.08in 0 0.03in;
}

main.resume p {
  font-size: 10pt;
  line-height: 1.4;
  margin: 0.04in 0;
  text-align: justify;
}

main.resume ul, main.resume ol {
  margin: 0.05in 0 0.1in 1em;
  padding: 0;
}

main.resume li {
  font-size: 10pt;
  line-height: 1.35;
  margin: 0.025in 0;
}

main.resume table {
  width: 100%;
  border-collapse: collapse;
  font-size: 9.5pt;
  margin: 0.1in 0;
}

main.resume th, main.resume td {
  border: 0.5pt solid #999;
  padding: 0.05in 0.08in;
  text-align: left;
}

main.resume th {
  background: #f5f5f5;
  font-weight: 700;
}
`,
  },
  {
    id: "nsf-grfp",
    name: "NSF GRFP",
    description: "NSF Graduate Research Fellowship Program layout with research and personal statements.",
    category: "funder",
    page_size: "letter",
    scaffold: NSF_GRFP_SCAFFOLD,
    css: `@page { margin: 1in; }

main.resume { display: block; color: #1a1a1a; }

main.resume h1 {
  font-size: 16pt;
  font-weight: 700;
  text-align: center;
  margin: 0 0 0.15in;
}

main.resume h1 + p {
  text-align: center;
  font-size: 10pt;
  color: #2a6fb0;
  margin: 0 0 0.25in;
}

main.resume h2 {
  font-size: 11pt;
  font-weight: 700;
  color: #2a6fb0;
  margin: 0.22in 0 0.08in;
}

main.resume h3 {
  font-size: 10.5pt;
  font-weight: 700;
  margin: 0.12in 0 0.04in;
}

main.resume p {
  font-size: 10pt;
  line-height: 1.45;
  margin: 0.04in 0;
}

main.resume ul, main.resume ol {
  margin: 0.04in 0 0.1in 1em;
  padding: 0;
}

main.resume li {
  font-size: 10pt;
  line-height: 1.35;
  margin: 0.025in 0;
}

main.resume table {
  width: 100%;
  border-collapse: collapse;
  font-size: 9.5pt;
  margin: 0.1in 0;
}

main.resume th, main.resume td {
  border-bottom: 0.5pt solid #ccc;
  padding: 0.05in 0.08in;
}

main.resume th {
  font-weight: 700;
  border-bottom: 1pt solid #2a6fb0;
}
`,
  },
  {
    id: "erc-starting",
    name: "ERC Starting Grant",
    description: "ERC-style proposal with work packages, impact, and risk management sections.",
    category: "funder",
    page_size: "a4",
    scaffold: ERC_STARTING_SCAFFOLD,
    css: `@page { margin: 20mm; }

main.resume { display: block; }

main.resume h1 {
  font-size: 18pt;
  font-weight: 700;
  margin: 0 0 0.15in;
}

main.resume h1 + p {
  font-size: 10pt;
  color: #444;
  margin: 0 0 0.25in;
}

main.resume h2 {
  font-size: 12pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin: 0.25in 0 0.08in;
}

main.resume h3 {
  font-size: 11pt;
  font-weight: 700;
  color: #333;
  margin: 0.14in 0 0.04in;
}

main.resume h4 {
  font-size: 10.5pt;
  font-weight: 600;
  font-style: italic;
  margin: 0.08in 0 0.03in;
}

main.resume p {
  font-size: 11pt;
  line-height: 1.4;
  margin: 0.04in 0;
}

main.resume ul, main.resume ol {
  margin: 0.05in 0 0.1in 1em;
  padding: 0;
}

main.resume li {
  font-size: 11pt;
  line-height: 1.35;
  margin: 0.03in 0;
}

main.resume table {
  width: 100%;
  border-collapse: collapse;
  font-size: 10pt;
  margin: 0.1in 0;
}

main.resume th, main.resume td {
  border: 0.5pt solid #bbb;
  padding: 0.06in;
}

main.resume th {
  background: #f0f0f0;
  font-weight: 700;
}
`,
  },
  {
    id: "academic-proposal",
    name: "Academic proposal",
    description: "Generic academic research proposal: abstract, objectives, methodology, timeline, budget.",
    category: "layout",
    page_size: "letter",
    scaffold: ACADEMIC_PROPOSAL_SCAFFOLD,
    css: `@page { margin: 0.75in; }

main.resume { display: block; }

main.resume h1 {
  font-size: 18pt;
  font-weight: 700;
  margin: 0 0 0.1in;
}

main.resume h1 + p {
  font-size: 10pt;
  color: #444;
  margin: 0 0 0.2in;
}

main.resume h2 {
  font-size: 12pt;
  font-weight: 700;
  margin: 0.22in 0 0.08in;
  border-bottom: 0.75pt solid #aaa;
  padding-bottom: 0.03in;
}

main.resume h3 {
  font-size: 11pt;
  font-weight: 700;
  margin: 0.12in 0 0.04in;
}

main.resume p {
  font-size: 11pt;
  line-height: 1.4;
  margin: 0.04in 0;
}

main.resume ul, main.resume ol {
  margin: 0.05in 0 0.1in 1em;
  padding: 0;
}

main.resume li {
  font-size: 11pt;
  line-height: 1.35;
  margin: 0.025in 0;
}

main.resume table {
  width: 100%;
  border-collapse: collapse;
  font-size: 10pt;
  margin: 0.1in 0;
}

main.resume th, main.resume td {
  border: 0.5pt solid #999;
  padding: 0.05in 0.08in;
}

main.resume th {
  background: #f7f7f7;
  font-weight: 700;
}
`,
  },
  {
    id: "corporate-grant",
    name: "Corporate grant",
    description: "Grant application for foundations or corporate programs with executive summary and evaluation.",
    category: "layout",
    page_size: "letter",
    scaffold: CORPORATE_GRANT_SCAFFOLD,
    css: `@page { margin: 0.8in; }

main.resume { display: block; }

main.resume h1 {
  font-size: 20pt;
  font-weight: 700;
  color: #1a3c6c;
  margin: 0 0 0.12in;
}

main.resume h1 + p {
  font-size: 9.5pt;
  color: #555;
  margin: 0 0 0.22in;
}

main.resume h2 {
  font-size: 12pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #1a3c6c;
  margin: 0.24in 0 0.08in;
}

main.resume h3 {
  font-size: 11pt;
  font-weight: 700;
  margin: 0.12in 0 0.04in;
}

main.resume p {
  font-size: 10.5pt;
  line-height: 1.4;
  margin: 0.04in 0;
}

main.resume ul, main.resume ol {
  margin: 0.05in 0 0.1in 1em;
  padding: 0;
}

main.resume li {
  font-size: 10.5pt;
  line-height: 1.35;
  margin: 0.025in 0;
}

main.resume table {
  width: 100%;
  border-collapse: collapse;
  font-size: 10pt;
  margin: 0.1in 0;
}

main.resume th, main.resume td {
  border-bottom: 0.5pt solid #ccc;
  padding: 0.05in 0.08in;
}

main.resume th {
  font-weight: 700;
  color: #1a3c6c;
  border-bottom: 1pt solid #1a3c6c;
}
`,
  },
  {
    id: "two-column-proposal",
    name: "Two-column proposal",
    description: "Compact two-column layout useful for short proposals, concept notes, or one-pagers.",
    category: "layout",
    page_size: "letter",
    scaffold: ACADEMIC_PROPOSAL_SCAFFOLD,
    css: `@page { margin: 0.55in; }

main.resume {
  column-count: 2;
  column-gap: 0.4in;
  column-fill: auto;
}

/* Title sits in the first column; Paged.js is fragile with column-span
   in multi-column flows, so we keep the layout simple and stable. */
main.resume h1 { font-size: 18pt; font-weight: 700; margin: 0 0 0.05in; }
main.resume h1 + p { font-size: 9pt; color: #444; margin: 0 0 0.18in; }

main.resume h2 {
  font-size: 10.5pt;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-bottom: 0.5pt solid #999;
  padding-bottom: 0.02in;
  margin: 0.14in 0 0.06in;
}

main.resume h3 { font-size: 10pt; font-weight: 700; margin: 0.08in 0 0.02in; }
main.resume h4 { font-size: 9.5pt; font-style: italic; color: #555; margin: 0 0 0.03in; }
main.resume p { font-size: 9.5pt; line-height: 1.3; margin: 0.025in 0; }
main.resume ul, main.resume ol { margin: 0.03in 0 0.06in; padding-left: 0.22in; list-style-position: outside; }
main.resume li { font-size: 9.5pt; line-height: 1.25; margin: 0.015in 0; }
main.resume table { font-size: 8.5pt; width: 100%; border-collapse: collapse; margin: 0.06in 0; }
main.resume th, main.resume td { border: 0.5pt solid #bbb; padding: 0.04in; }
main.resume th { background: #f5f5f5; font-weight: 700; }
`,
  },
];

/** Return all templates, or filter by category. */
export function getProposalTemplates(category?: "funder" | "layout"): ProposalTemplate[] {
  if (!category) return PROPOSAL_TEMPLATES;
  return PROPOSAL_TEMPLATES.filter((t) => t.category === category);
}

/** Look up a proposal template by ID. */
export function getProposalTemplate(id: string): ProposalTemplate | undefined {
  return PROPOSAL_TEMPLATES.find((t) => t.id === id);
}

/** Return true if the template ID belongs to a proposal template. */
export function isProposalTemplate(id: string): boolean {
  return PROPOSAL_TEMPLATES.some((t) => t.id === id);
}
