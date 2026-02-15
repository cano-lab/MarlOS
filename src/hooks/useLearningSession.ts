import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export type LearningStep =
  | "topic"      // Learner picks a topic or asks a question
  | "predict"    // Learner makes prediction (required)
  | "answer"     // AI reveals answer (with optional web search)
  | "compare"    // AI compares prediction to answer
  | "integrate"  // Learner summarizes what they learned
  | "next";      // AI suggests follow-up questions

export interface LearningCycle {
  id: string;
  topic: string;
  prediction: string;
  predictionType: "confident" | "partial" | "guess" | "no_idea";
  answer: string;
  comparison: string;
  integration: string;
  followUpQuestions: string[];
  timestamp: number;
}

export interface LearningSessionState {
  step: LearningStep;
  currentCycle: Partial<LearningCycle>;
  history: LearningCycle[];
  isLoading: boolean;
  error: string | null;
}

// System prompts for each step
const SYSTEM_PROMPTS = {
  answer: `You are a learning assistant. The user has made a prediction about a topic.

Your job is to:
1. Provide a clear, accurate answer to their question
2. Be thorough but accessible
3. Include concrete examples where helpful
4. If the topic requires current information, indicate what might need verification

Do NOT compare to their prediction yet - just provide the accurate answer.`,

  compare: `You are a learning assistant analyzing a prediction.

The user predicted: {prediction}
The actual answer was: {answer}

Your job is to:
1. Acknowledge what they got RIGHT (even if partial)
2. Identify specific gaps or misconceptions
3. Explain WHY the gap exists (what assumption led them astray?)
4. Frame wrongness as valuable data, not failure

Be encouraging but honest. Wrong predictions are the best teachers.`,

  followUp: `You are a learning assistant helping generate curiosity.

Based on the topic "{topic}" and what was just learned, suggest 3-4 follow-up questions that:
1. Build on the new understanding
2. Explore edge cases or exceptions
3. Connect to related concepts
4. Go deeper into interesting aspects

Format as a simple list. Make them specific and intriguing.`,

  noModel: `The user said they don't know or have no idea about this topic.

Your job is to:
1. Normalize not knowing - it's the honest starting point
2. Build a MINIMAL mental model - just enough to make a prediction next time
3. Use analogies to connect to things they might know
4. End with a simple question they could now predict on

Keep it brief - just enough scaffolding to bootstrap curiosity.`
};

export function useLearningSession() {
  const [state, setState] = createSignal<LearningSessionState>({
    step: "topic",
    currentCycle: {},
    history: [],
    isLoading: false,
    error: null,
  });

  const generateId = () => Math.random().toString(36).substring(2, 9);

  // Search context for current cycle
  let currentSearchContext = "";

  // Set the topic/question
  const setTopic = (topic: string, searchContext?: string) => {
    currentSearchContext = searchContext || "";
    setState(s => ({
      ...s,
      step: "predict",
      currentCycle: {
        id: generateId(),
        topic,
        timestamp: Date.now(),
      },
      error: null,
    }));
  };

  // Submit prediction and get answer
  const submitPrediction = async (prediction: string, type: LearningCycle["predictionType"]) => {
    setState(s => ({
      ...s,
      currentCycle: { ...s.currentCycle, prediction, predictionType: type },
      isLoading: true,
      error: null,
    }));

    try {
      const topic = state().currentCycle.topic!;

      // Handle "no idea" case differently
      let systemPrompt = SYSTEM_PROMPTS.answer;
      let userMessage = `Topic/Question: ${topic}`;

      // Add search context if available
      if (currentSearchContext) {
        userMessage += `\n\n[Research Context]${currentSearchContext}`;
      }

      if (type === "no_idea") {
        systemPrompt = SYSTEM_PROMPTS.noModel;
        userMessage = `The user wants to learn about: ${topic}\n\nThey said: "${prediction}"`;
        if (currentSearchContext) {
          userMessage += `\n\n[Research Context]${currentSearchContext}`;
        }
      }

      // Call AI for answer (optionally with web search in future)
      const answer = await callAI(systemPrompt, userMessage);

      setState(s => ({
        ...s,
        step: "answer",
        currentCycle: { ...s.currentCycle, answer },
        isLoading: false,
      }));
    } catch (e) {
      setState(s => ({
        ...s,
        isLoading: false,
        error: `Failed to get answer: ${e}`,
      }));
    }
  };

  // Move to comparison step
  const requestComparison = async () => {
    setState(s => ({ ...s, isLoading: true, error: null }));

    try {
      const { prediction, answer, topic, predictionType } = state().currentCycle;

      // Skip comparison for "no idea" - they got the scaffolding already
      if (predictionType === "no_idea") {
        setState(s => ({
          ...s,
          step: "compare",
          currentCycle: {
            ...s.currentCycle,
            comparison: "Since you started with no model, the explanation above is your foundation. Now you have enough to make predictions!"
          },
          isLoading: false,
        }));
        return;
      }

      const systemPrompt = SYSTEM_PROMPTS.compare
        .replace("{prediction}", prediction!)
        .replace("{answer}", answer!);

      const comparison = await callAI(systemPrompt, `Analyze this prediction for the topic: ${topic}`);

      setState(s => ({
        ...s,
        step: "compare",
        currentCycle: { ...s.currentCycle, comparison },
        isLoading: false,
      }));
    } catch (e) {
      setState(s => ({
        ...s,
        isLoading: false,
        error: `Failed to generate comparison: ${e}`,
      }));
    }
  };

  // Submit integration (what the learner learned)
  const submitIntegration = async (integration: string) => {
    setState(s => ({
      ...s,
      currentCycle: { ...s.currentCycle, integration },
      isLoading: true,
      error: null,
    }));

    try {
      const topic = state().currentCycle.topic!;
      const systemPrompt = SYSTEM_PROMPTS.followUp.replace("{topic}", topic);

      const followUpResponse = await callAI(systemPrompt,
        `Topic: ${topic}\nWhat was learned: ${integration}`);

      // Parse follow-up questions (simple line split)
      const followUpQuestions = followUpResponse
        .split("\n")
        .map(q => q.replace(/^[\d\-\*\.\)]+\s*/, "").trim())
        .filter(q => q.length > 10);

      setState(s => ({
        ...s,
        step: "next",
        currentCycle: { ...s.currentCycle, followUpQuestions },
        isLoading: false,
      }));
    } catch (e) {
      setState(s => ({
        ...s,
        isLoading: false,
        error: `Failed to generate follow-up questions: ${e}`,
      }));
    }
  };

  // Start a new cycle (optionally with a follow-up question)
  const startNewCycle = (topic?: string) => {
    // Save current cycle to history if complete
    const current = state().currentCycle;
    if (current.topic && current.answer) {
      setState(s => ({
        ...s,
        history: [...s.history, current as LearningCycle],
      }));
    }

    if (topic) {
      setTopic(topic);
    } else {
      setState(s => ({
        ...s,
        step: "topic",
        currentCycle: {},
        error: null,
      }));
    }
  };

  // Reset everything
  const reset = () => {
    setState({
      step: "topic",
      currentCycle: {},
      history: [],
      isLoading: false,
      error: null,
    });
  };

  return {
    state,
    setTopic,
    submitPrediction,
    requestComparison,
    submitIntegration,
    startNewCycle,
    reset,
  };
}

// Helper to call the AI
async function callAI(systemPrompt: string, userMessage: string): Promise<string> {
  try {
    const response = await invoke<{ content: string }>("ai_chat", {
      systemPrompt,
      message: userMessage,
    });
    return response.content;
  } catch (e) {
    // Fallback: try without system prompt if command doesn't support it
    const response = await invoke<{ content: string }>("ai_generate", {
      prompt: `${systemPrompt}\n\n---\n\n${userMessage}`,
    });
    return response.content;
  }
}

// Web search types
export interface SearchResult {
  title: string;
  url: string;
  snippet: string;
  source_domain: string;
}

export interface SearchResults {
  query: string;
  results: SearchResult[];
  total_found: number;
}

interface AcademicPaper {
  title: string;
  authors: string[];
  year: number | null;
  abstract_text: string | null;
  url: string;
  pdf_url: string | null;
  citation_count: number | null;
  source: string;
  doi: string | null;
  venue: string | null;
}

interface AcademicSearchResults {
  query: string;
  papers: AcademicPaper[];
  total_found: number;
}

// Web search helper
export async function webSearch(query: string, numResults: number = 5): Promise<SearchResults> {
  try {
    const results = await invoke<SearchResults>("mcp_web_search", {
      query,
      numResults
    });
    return results;
  } catch (e) {
    return { query, results: [], total_found: 0, error: String(e) };
  }
}

// Academic search helper
export async function academicSearch(query: string, numResults: number = 5): Promise<AcademicSearchResults> {
  try {
    const results = await invoke<AcademicSearchResults>("mcp_research_academic", {
      query,
      numResults
    });
    return results;
  } catch (e) {
    return { query, papers: [], total_found: 0, error: String(e) };
  }
}

// Format search results for AI context
export function formatSearchResultsForContext(results: SearchResults): string {
  if (results.results.length === 0) return "";

  let context = `\n\n[Web Search Results for "${results.query}"]\n`;
  for (const r of results.results) {
    context += `- ${r.title} (${r.source_domain}): ${r.snippet}\n`;
  }
  return context;
}

// Format academic results for AI context
export function formatAcademicResultsForContext(results: AcademicSearchResults): string {
  if (results.papers.length === 0) return "";

  let context = `\n\n[Academic Papers for "${results.query}"]\n`;
  for (const p of results.papers) {
    const authors = p.authors.slice(0, 2).join(", ") + (p.authors.length > 2 ? " et al." : "");
    const year = p.year ? ` (${p.year})` : "";
    const citations = p.citation_count ? ` [${p.citation_count} citations]` : "";
    context += `- ${p.title}${year} by ${authors}${citations}\n`;
    if (p.abstract_text) {
      context += `  Abstract: ${p.abstract_text.slice(0, 200)}...\n`;
    }
  }
  return context;
}
