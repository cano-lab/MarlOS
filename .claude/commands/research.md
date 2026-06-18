# Research & Summarize Papers

Research a topic by searching academic databases and the web, fetching full paper content, and producing a structured summary with citations.

## Instructions

The user wants to research: $ARGUMENTS

Follow these steps:

### Step 1: Search for papers and web sources

Run these MCP tool calls in parallel:
- `research_papers` with the user's query (8 results)
- `web_search` with the user's query (5 results)

### Step 2: Select the best sources

From the search results, pick the **top 3-5 most relevant papers** based on:
- Relevance to the query
- Citation count (prefer well-cited papers)
- Recency (prefer recent work unless it's a foundational topic)
- Having an abstract or URL available

Also pick **1-2 web sources** if they add value (tutorials, blog posts, documentation).

### Step 3: Fetch full content

Use `fetch_page` to retrieve the full text of each selected source. Run these in parallel where possible. If a paper has a `pdf_url`, try that first; otherwise use the regular `url`.

### Step 4: Synthesize and summarize

Write a structured research summary with these sections:

**Overview** — 2-3 sentence summary of the topic and what the literature says.

**Key Findings** — Bullet points of the most important findings across all sources. Cite each finding with (Author, Year) or [Source Name].

**Paper Summaries** — For each paper/source, provide:
- Title, authors, year
- 3-5 sentence summary of the paper's contribution
- Key methodology or approach
- Main results or conclusions

**Open Questions & Gaps** — What remains unresolved or debated in the literature.

**Suggested Reading Order** — If someone wanted to learn this topic, which paper should they read first and why.

### Step 5: Save to memory

Use `search_memory` to check if similar research has been done before. If this is a new topic, use `log_decision` to record that this research was conducted, with the key findings as reasoning.

## Important

- Always cite specific papers by author and year
- Don't make up information — only report what's in the fetched content
- If fetch_page fails for a URL, fall back to the abstract from the search results
- Keep the total summary under 2000 words unless the user asks for more detail
