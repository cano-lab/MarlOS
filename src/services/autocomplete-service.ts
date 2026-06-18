/**
 * Smart Autocomplete Service
 *
 * Philosophy: Assist, don't replace.
 * - Short suggestions (1-10 words)
 * - Context-aware (code vs prose vs academic)
 * - Conservative triggering (only when stuck)
 * - Feels like a thinking partner, not a ghost writer
 */

import { nGramAutocomplete } from './autocomplete-ngram';
import { autocompleteModel } from './autocomplete-model';

export type ContentType = 'code' | 'prose' | 'academic' | 'markdown' | 'unknown';

export interface AutocompleteContext {
  textBefore: string;
  textAfter: string;
  contentType: ContentType;
  fileExtension?: string;
  recentEdits: number; // How much user has typed recently
  pauseDuration: number; // ms since last keystroke

  // Enhanced context
  currentSection?: string; // Current heading/section name
  inList?: boolean; // Are we in a bulleted/numbered list?
  inCodeBlock?: boolean; // Are we in a code block?
  inQuote?: boolean; // Are we in a blockquote?
  documentTitle?: string; // Document/file name
  recentDocuments?: string[]; // Recently viewed documents
  semanticContext?: string; // Semantically similar content
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
  private useFineTunedModel = false;

  /**
   * Set whether to use fine-tuned model for autocomplete
   */
  setUseFineTunedModel(use: boolean) {
    this.useFineTunedModel = use;
    autocompleteModel.setUseFineTunedModel(use);
  }

  /**
   * Check if fine-tuned model is available
   */
  hasFineTunedModel(): boolean {
    return autocompleteModel.getFineTunedModels().length > 0;
  }

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
    const { textBefore, pauseDuration } = context;

    console.log('[Autocomplete] shouldTrigger:', {
      pauseDuration,
      textLength: textBefore.length,
      lastTrigger: Date.now() - this.lastTriggerTime,
      rejections: this.consecutiveRejections,
    });

    // Don't trigger if user hasn't paused long enough
    if (pauseDuration < 750) return false;

    // Don't trigger too frequently (reduced to 500ms for more responsive feel)
    const now = Date.now();
    if (now - this.lastTriggerTime < 500) {
      console.log('[Autocomplete] Blocked: too soon since last trigger');
      return false;
    }

    // Cooldown after rejections
    if (this.consecutiveRejections > 2) {
      if (now - this.lastTriggerTime < 5000) {
        console.log('[Autocomplete] Blocked: rejection cooldown');
        return false;
      }
      this.consecutiveRejections = 0;
    }

    // Need at least some context
    if (textBefore.trim().length < 3) {
      console.log('[Autocomplete] Blocked: too little context');
      return false;
    }

    console.log('[Autocomplete] ✓ Approved');
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

    // Try fine-tuned model first if enabled and available
    if (this.useFineTunedModel && this.hasFineTunedModel()) {
      try {
        console.log('[Autocomplete] Getting fine-tuned model completion for:', context.textBefore.slice(-50));

        const text = await autocompleteModel.complete(context.textBefore, 8);

        console.log('[Autocomplete] Fine-tuned model result:', text);

        if (text) {
          const cleaned = this.cleanSuggestion(text, context);
          console.log('[Autocomplete] Cleaned suggestion:', cleaned);

          if (cleaned) {
            return {
              text: cleaned,
              confidence: 0.85, // Higher confidence for fine-tuned model
              type: 'completion',
              reason: 'Continue writing (personalized)',
            };
          }
        }
      } catch (e) {
        console.warn('[Autocomplete] Fine-tuned model failed, falling back to n-gram:', e);
      }
    }

    // Try semantic similarity search for context-aware suggestions
    let semanticContext: string | undefined;
    if (context.currentSection) {
      semanticContext = `Section: ${context.currentSection}`;
    }

    // Fall back to n-gram autocomplete with enhanced structure context
    try {
      console.log('[Autocomplete] Getting n-gram completion with structure context');

      const text = await nGramAutocomplete.complete(
        context.textBefore,
        8,
        {
          inList: context.inList ?? false,
          inCodeBlock: context.inCodeBlock ?? false,
          inQuote: context.inQuote ?? false,
          currentHeading: context.currentSection,
        },
        semanticContext
      );

      console.log('[Autocomplete] N-gram result:', text);

      if (!text) {
        console.log('[Autocomplete] No suggestion returned');
        return null;
      }

      // Clean up the suggestion
      const cleaned = this.cleanSuggestion(text, context);
      console.log('[Autocomplete] Cleaned suggestion:', cleaned);

      if (!cleaned) return null;

      return {
        text: cleaned,
        confidence: 0.75, // Slightly higher with enhanced context
        type: 'completion',
        reason: 'Continue writing',
      };
    } catch (e) {
      console.error('[Autocomplete] Request failed:', e);
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

  private cleanSuggestion(text: string, context: AutocompleteContext): string | null {
    // Clean up the suggestion
    let cleaned = text.trim();

    // Don't suggest if it's too short or empty
    if (cleaned.length < 2) return null;

    // Don't suggest if it repeats the last word the user typed
    const lastWord = context.textBefore.split(/\s+/).pop()?.toLowerCase();
    const firstSuggestedWord = cleaned.split(/\s+/)[0]?.toLowerCase();
    if (lastWord && firstSuggestedWord === lastWord) {
      cleaned = cleaned.split(/\s+/).slice(1).join(' ');
    }

    // Empty after removing duplicate
    if (cleaned.length < 2) return null;

    return cleaned;
  }

}

// Singleton instance
export const autocompleteService = new AutocompleteService();
