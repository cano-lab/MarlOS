/**
 * Smart Autocomplete Service
 * 
 * Philosophy: Assist, don't replace.
 * - Short suggestions (1-10 words)
 * - Context-aware (code vs prose vs academic)
 * - Conservative triggering (only when stuck)
 * - Feels like a thinking partner, not a ghost writer
 */

import { aiProviderManager } from './ai-config';

export type ContentType = 'code' | 'prose' | 'academic' | 'markdown' | 'unknown';

export interface AutocompleteContext {
  textBefore: string;
  textAfter: string;
  contentType: ContentType;
  fileExtension?: string;
  recentEdits: number; // How much user has typed recently
  pauseDuration: number; // ms since last keystroke
}

export interface AutocompleteSuggestion {
  text: string;
  confidence: number;
  type: 'completion' | 'snippet' | 'correction';
  reason: string; // Why this suggestion was made
}

// Code language detection patterns
const CODE_PATTERNS: Record<string, RegExp> = {
  javascript: /\b(const|let|var|function|=>|import|export|class)\b/,
  typescript: /\b(interface|type|:\s*(string|number|boolean))\b/,
  python: /\b(def|class|import|from|if\s+__name__)\b/,
  rust: /\b(fn|let\s+mut|impl|struct|enum|use)\b/,
  go: /\b(func|package|import|struct|interface)\b/,
  markdown: /\[.*?\]\(.*?\)|#+\s|^\s*[-*+]\s|^\s*\d+\./,
};

// Academic writing patterns
const ACADEMIC_PATTERNS = [
  /\b(however|therefore|furthermore|moreover|consequently)\b/i,
  /\b(et\s+al\.?|i\.e\.?|e\.g\.?|cf\.?|viz\.?)\b/i,
  /\b(study|research|analysis|hypothesis|methodology)\b/i,
  /\(\d{4}\)/, // Citations like (Smith, 2024)
  /\[\d+\]/, // Numbered citations [1]
];

class AutocompleteService {
  private lastTriggerTime = 0;
  private consecutiveRejections = 0;

  /**
   * Detect the type of content being written
   */
  detectContentType(context: Pick<AutocompleteContext, 'textBefore' | 'fileExtension'>): ContentType {
    const { textBefore, fileExtension } = context;
    const sample = textBefore.slice(-500).toLowerCase();

    // Check file extension first
    if (fileExtension) {
      const ext = fileExtension.toLowerCase();
      if (['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h'].includes(ext)) {
        return 'code';
      }
      if (['md', 'markdown'].includes(ext)) {
        return 'markdown';
      }
    }

    // Check for code patterns
    for (const [lang, pattern] of Object.entries(CODE_PATTERNS)) {
      if (pattern.test(sample)) {
        if (lang === 'markdown') return 'markdown';
        return 'code';
      }
    }

    // Check for academic patterns
    const academicScore = ACADEMIC_PATTERNS.reduce((score, pattern) => {
      return score + (pattern.test(sample) ? 1 : 0);
    }, 0);
    if (academicScore >= 2) {
      return 'academic';
    }

    // Default to prose
    return 'prose';
  }

  /**
   * Determine if we should trigger autocomplete
   * 
   * Rules:
   * - User has paused typing (not mid-flow)
   * - At a natural break point (end of word, not mid-word)
   * - Not after a rejection (cooldown period)
   * - Context has enough information to be helpful
   */
  shouldTrigger(context: AutocompleteContext): boolean {
    const { textBefore, pauseDuration, recentEdits } = context;

    // Don't trigger if user is typing fast (in flow)
    if (pauseDuration < 600) {
      return false;
    }

    // Don't trigger too frequently
    const now = Date.now();
    if (now - this.lastTriggerTime < 2000) {
      return false;
    }

    // Cooldown after rejections
    if (this.consecutiveRejections > 2) {
      if (now - this.lastTriggerTime < 5000) {
        return false;
      }
      this.consecutiveRejections = 0;
    }

    // Need meaningful context
    const lastSentence = this.getLastSentence(textBefore);
    if (lastSentence.trim().length < 10) {
      return false;
    }

    // Don't trigger mid-word
    const lastChar = textBefore.slice(-1);
    if (/[a-zA-Z0-9_]/.test(lastChar)) {
      // Check if we're at a natural word boundary
      const lastFewChars = textBefore.slice(-3);
      if (!/[\s.,;:!?]/.test(lastFewChars)) {
        return false;
      }
    }

    // Don't trigger after recent large edits (user is restructuring)
    if (recentEdits > 50) {
      return false;
    }

    return true;
  }

  /**
   * Get autocomplete suggestion
   */
  async getSuggestion(context: AutocompleteContext): Promise<AutocompleteSuggestion | null> {
    if (!this.shouldTrigger(context)) {
      return null;
    }

    this.lastTriggerTime = Date.now();

    const contentType = context.contentType === 'unknown' 
      ? this.detectContentType(context) 
      : context.contentType;

    try {
      const prompt = this.buildPrompt(context, contentType);
      
      const response = await fetch(
        aiProviderManager.getUrl('/chat/completions'),
        {
          method: 'POST',
          headers: aiProviderManager.getHeaders(),
          body: JSON.stringify({
            model: aiProviderManager.getActiveProvider().defaultModel,
            messages: [
              { 
                role: 'system', 
                content: this.getSystemPrompt(contentType) 
              },
              { role: 'user', content: prompt }
            ],
            temperature: 0.2, // Lower for predictable completions
            max_tokens: 20,   // Short completions only
          }),
        }
      );

      if (!response.ok) {
        throw new Error(`AI request failed: ${response.status}`);
      }

      const data = await response.json();
      const text = data.choices?.[0]?.message?.content?.trim();

      if (!text || text.length < 2) {
        return null;
      }

      // Clean up the suggestion
      const cleaned = this.cleanSuggestion(text, context);
      if (!cleaned) {
        return null;
      }

      return {
        text: cleaned,
        confidence: 0.8,
        type: 'completion',
        reason: this.getReason(contentType),
      };
    } catch (e) {
      console.error('Autocomplete failed:', e);
      return null;
    }
  }

  /**
   * Record that user accepted a suggestion
   */
  recordAcceptance() {
    this.consecutiveRejections = 0;
  }

  /**
   * Record that user rejected a suggestion
   */
  recordRejection() {
    this.consecutiveRejections++;
  }

  private getSystemPrompt(contentType: ContentType): string {
    const base = `You are a writing assistant. Provide SHORT completions (2-8 words max) to help the user continue their thought.

Rules:
- Complete the current phrase, not the entire sentence
- Match the user's style and tone
- Never repeat what the user already typed
- Prefer natural phrasings over formal ones
- When uncertain, provide shorter completions`;

    const specifics: Record<ContentType, string> = {
      code: `${base}

For code:
- Complete variable names, function calls, or expressions
- Suggest appropriate method names or parameters
- Follow language conventions`,
      
      academic: `${base}

For academic writing:
- Use appropriate transitional phrases
- Maintain formal tone but avoid being verbose
- Suggest citations or references only if clearly relevant`,
      
      prose: `${base}

For general writing:
- Keep it conversational and natural
- Complete the thought without over-explaining`,
      
      markdown: `${base}

For Markdown:
- Suggest link text or formatting
- Complete list items naturally
- Help with heading structure`,
      
      unknown: base,
    };

    return specifics[contentType] || base;
  }

  private buildPrompt(context: AutocompleteContext, contentType: ContentType): string {
    const { textBefore, textAfter } = context;
    
    // Get the most relevant context (last sentence/line)
    const relevantBefore = this.getLastSentence(textBefore).slice(-200);
    const relevantAfter = textAfter.slice(0, 50);

    return `Complete this ${contentType} text with a SHORT continuation (2-8 words):

Text before:
${relevantBefore}|

Text after:
${relevantAfter}

Provide only the completion (the part that goes where | is), nothing else:`;
  }

  private cleanSuggestion(text: string, context: AutocompleteContext): string | null {
    // Remove common prefixes the AI might add
    let cleaned = text
      .replace(/^(complete|continuation|suggestion):\s*/i, '')
      .replace(/^["']|["']$/g, '')
      .trim();

    // Don't suggest if it's too long (likely over-eager AI)
    const words = cleaned.split(/\s+/);
    if (words.length > 10) {
      cleaned = words.slice(0, 8).join(' ');
    }

    // Don't suggest if it repeats what user already typed
    const lastWords = context.textBefore.toLowerCase().split(/\s+/).slice(-5).join(' ');
    if (cleaned.toLowerCase().startsWith(lastWords)) {
      cleaned = cleaned.slice(lastWords.length).trim();
    }

    // Ensure it ends naturally
    if (!/[.!?;:,\s]$/.test(cleaned)) {
      // Don't add punctuation, just ensure there's a clean ending
      cleaned = cleaned.replace(/\s+$/g, '');
    }

    return cleaned.length > 0 ? cleaned : null;
  }

  private getLastSentence(text: string): string {
    // Find the last sentence break
    const breaks = /[.!?\n]/;
    const lastBreak = Math.max(
      text.lastIndexOf('.'),
      text.lastIndexOf('!'),
      text.lastIndexOf('?'),
      text.lastIndexOf('\n')
    );
    
    if (lastBreak === -1) {
      return text;
    }
    
    return text.slice(lastBreak + 1);
  }

  private getReason(contentType: ContentType): string {
    const reasons: Record<ContentType, string> = {
      code: 'Complete the expression',
      academic: 'Continue the thought',
      prose: 'Finish the phrase',
      markdown: 'Complete the formatting',
      unknown: 'Continue writing',
    };
    return reasons[contentType];
  }
}

// Singleton instance
export const autocompleteService = new AutocompleteService();
