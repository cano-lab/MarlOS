# Paper Generation Pipeline Call Graph

## Overview
This documents the complete function call chain for generating research papers from sources in MarlOS.

## Pipeline Stages
1. Create paper
2. Add sources
3. Chunk sources
4. Extract findings
5. Generate outline
6. Write sections
7. Export

## Call Graph

```
paper_create(title, question, type, style)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ commands.rs:2302 → pipeline.rs:135                                       │
│ PaperPipeline::new(paper, search, ai_manager)                            │
│     │                                                                    │
│     └──► Paper::new(title, question)                                     │
│              .with_type(paper_type)                                      │
│              .with_citation_style(style)                                 │
└──────────────────────────────────────────────────────────────────────────┘
     │
     ▼
paper_add_sources(paper_id, source_ids)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ pipeline.rs:162                                                          │
│ add_sources(source_ids) -> usize                                         │
│     │                                                                    │
│     ├──► for each source_id:                                             │
│     │        load_source(id) ───────────────────────────────────────┐   │
│     │                                                                │   │
│     │    ┌───────────────────────────────────────────────────────────┘   │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ pipeline.rs:181                                             │  │
│     │  │ load_source(id) -> Option<Source>                           │  │
│     │  │     │                                                       │  │
│     │  │     └──► semantic_search.get(suid)                          │  │
│     │  │              → Deserialize object.content to Source         │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     └──► paper.add_sources(source_ids)                                   │
└──────────────────────────────────────────────────────────────────────────┘
     │
     ▼
paper_chunk_sources(paper_id)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ pipeline.rs:204                                                          │
│ chunk_sources() -> ChunkingProgress                                      │
│     │                                                                    │
│     ├──► SemanticChunker::new() ────────────────────────────────────┐   │
│     │                                                                │   │
│     │    ┌───────────────────────────────────────────────────────────┘   │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ chunk.rs:141                                                │  │
│     │  │ SemanticChunker with config:                                │  │
│     │  │   target_chunk_size: 1000 chars                             │  │
│     │  │   min_chunk_size: 200 chars                                 │  │
│     │  │   overlap: 100 chars                                        │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     └──► for each source:                                                │
│              chunker.chunk(source_id, content) ────────────────────┐    │
│                                                                     │    │
│          ┌──────────────────────────────────────────────────────────┘    │
│          ▼                                                               │
│        ┌─────────────────────────────────────────────────────────────┐  │
│        │ chunk.rs:152                                                │  │
│        │ chunk(source_id, content) -> Vec<SemanticChunk>             │  │
│        │     │                                                       │  │
│        │     ├──► Split at paragraph boundaries                      │  │
│        │     ├──► Detect chunk types (Abstract, Method, Results...)  │  │
│        │     ├──► Add overlap for context continuity                 │  │
│        │     └──► Assign page numbers if available                   │  │
│        └─────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
     │
     ▼
paper_extract_findings(paper_id)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ pipeline.rs:234                                                          │
│ extract_findings() -> ExtractionProgress                                 │
│     │                                                                    │
│     └──► for each chunk:                                                 │
│              extract_findings_from_chunk(chunk, question) ─────────┐    │
│                                                                     │    │
│          ┌──────────────────────────────────────────────────────────┘    │
│          ▼                                                               │
│        ┌─────────────────────────────────────────────────────────────┐  │
│        │ pipeline.rs:273                                             │  │
│        │ extract_findings_from_chunk(chunk, question)                │  │
│        │     │                                                       │  │
│        │     ├──► Build LLM prompt:                                  │  │
│        │     │        "Extract key findings from this text..."       │  │
│        │     │        "Research question: {question}"                │  │
│        │     │        "Text: {chunk.content}"                        │  │
│        │     │                                                       │  │
│        │     ├──► ai_manager.chat(prompt) ──────────────────────┐    │  │
│        │     │                                                   │    │  │
│        │     │    ┌──────────────────────────────────────────────┘    │  │
│        │     │    ▼                                                   │  │
│        │     │  ┌────────────────────────────────────────────────┐   │  │
│        │     │  │ ai.rs:231                                      │   │  │
│        │     │  │ chat(messages, system_prompt) -> AiResponse    │   │  │
│        │     │  │     │                                          │   │  │
│        │     │  │     └──► HTTP POST to LM Studio                │   │  │
│        │     │  │              localhost:1234/v1/chat/completions│   │  │
│        │     │  └────────────────────────────────────────────────┘   │  │
│        │     │                                                       │  │
│        │     └──► Parse JSON response into Vec<KeyFinding>           │  │
│        │              KeyFinding { type, content, source_id, ...}    │  │
│        └─────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
     │
     ▼
paper_generate_outline(paper_id, thesis_hint)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ pipeline.rs:378                                                          │
│ generate_outline(thesis_hint) -> Vec<PaperSection>                       │
│     │                                                                    │
│     ├──► Build prompt with:                                              │
│     │        - Research question                                         │
│     │        - Paper type (essay, research, review...)                   │
│     │        - All key findings                                          │
│     │        - Thesis hint if provided                                   │
│     │                                                                    │
│     ├──► ai_manager.chat(prompt) → JSON outline                          │
│     │                                                                    │
│     └──► Create PaperSection for each:                                   │
│              Introduction, Literature Review, Methods,                   │
│              Results, Discussion, Conclusion                             │
└──────────────────────────────────────────────────────────────────────────┘
     │
     ▼
paper_write_all(paper_id)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ pipeline.rs:572                                                          │
│ write_all_sections() -> WritingProgress                                  │
│     │                                                                    │
│     └──► for each section in order:                                      │
│              write_section(section_id) ────────────────────────────┐    │
│                                                                     │    │
│          ┌──────────────────────────────────────────────────────────┘    │
│          ▼                                                               │
│        ┌─────────────────────────────────────────────────────────────┐  │
│        │ pipeline.rs:504                                             │  │
│        │ write_section(section_id) -> PaperSection                   │  │
│        │     │                                                       │  │
│        │     ├──► Gather context:                                    │  │
│        │     │        - Section outline                              │  │
│        │     │        - Relevant key findings                        │  │
│        │     │        - Previous sections (for flow)                 │  │
│        │     │        - Available sources                            │  │
│        │     │                                                       │  │
│        │     ├──► Build prompt:                                      │  │
│        │     │        "Write the {section_type} section..."          │  │
│        │     │        "Use citations in format [[source_id:page]]"   │  │
│        │     │                                                       │  │
│        │     ├──► ai_manager.chat(prompt)                            │  │
│        │     │                                                       │  │
│        │     └──► section.set_content(response)                      │  │
│        │              section.status = Written                       │  │
│        └─────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
     │
     ▼
paper_export(paper_id, format)
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ export.rs:85                                                             │
│ PaperExporter::export(paper, sources, options)                           │
│     │                                                                    │
│     ├──► citation_tracker.rs:236                                         │
│     │    process_paper_citations(paper, sources)                         │
│     │        │                                                           │
│     │        ├──► Parse [[source_id:page]] markers                       │
│     │        ├──► Assign citation numbers                                │
│     │        └──► Replace markers with formatted citations               │
│     │                                                                    │
│     └──► match format:                                                   │
│              "markdown" → export_markdown(paper, options)                │
│              "html"     → export_html(paper, options)                    │
│              "text"     → export_plaintext(paper, options)               │
│                                                                          │
│     Returns: formatted paper with citations + bibliography               │
└──────────────────────────────────────────────────────────────────────────┘
```

## Key Functions

| Function | File:Line | Purpose |
|----------|-----------|---------|
| `PaperPipeline::new` | pipeline.rs:135 | Initialize pipeline |
| `add_sources` | pipeline.rs:162 | Load sources into paper |
| `chunk_sources` | pipeline.rs:204 | Split sources into chunks |
| `SemanticChunker::chunk` | chunk.rs:152 | Semantic chunking logic |
| `extract_findings` | pipeline.rs:234 | LLM extracts key points |
| `generate_outline` | pipeline.rs:378 | LLM creates structure |
| `write_section` | pipeline.rs:504 | LLM writes one section |
| `write_all_sections` | pipeline.rs:572 | Orchestrate all writing |
| `PaperExporter::export` | export.rs:85 | Format final output |
| `process_paper_citations` | citation_tracker.rs:236 | Handle citations |

## Data Structures

```rust
Paper {
    id: String,
    title: String,
    research_question: String,
    thesis: Option<String>,
    paper_type: PaperType,          // Essay, Research, Review, etc.
    citation_style: CitationStyle,   // APA, MLA, Chicago, etc.
    source_ids: Vec<String>,
    findings: Vec<KeyFinding>,
    sections: Vec<PaperSection>,
    status: PaperStatus,
}

KeyFinding {
    id: String,
    finding_type: FindingType,      // Quote, Data, Claim, Method
    content: String,
    source_id: String,
    location: Option<String>,       // Page number
    relevance: f32,
    used_in_sections: Vec<String>,
}

PaperSection {
    id: String,
    title: String,
    section_type: SectionType,      // Intro, Methods, Results, etc.
    content: String,
    citations: Vec<Citation>,
    word_count: usize,
    status: SectionStatus,
}
```

## Citation Flow

```
LLM writes: "Studies show positive results [[src123:p42]]"
                                              │
                                              ▼
CitationTracker.parse_citations() → Citation { source_id: "src123", page: "42" }
                                              │
                                              ▼
CitationTracker.assign_numbers() → citation_number: 1
                                              │
                                              ▼
CitationTracker.render_citation() → "(Smith, 2023, p. 42)" [APA]
                                    or "[1]" [IEEE]
                                              │
                                              ▼
Final: "Studies show positive results (Smith, 2023, p. 42)"
```

## Export Formats

| Format | Function | Output |
|--------|----------|--------|
| Markdown | `export_markdown` | `.md` with headers, citations |
| HTML | `export_html` | Styled HTML document |
| Plain Text | `export_plaintext` | Clean text, no formatting |
