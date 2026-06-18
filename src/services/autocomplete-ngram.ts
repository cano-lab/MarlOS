/**
 * Enhanced N-gram Autocomplete Service
 * Uses statistical patterns from your own text + document structure + user patterns
 */

interface NGramEntry {
  [key: string]: number; // word -> frequency
}

interface DocumentStructure {
  inList: boolean;      // In a bulleted/numbered list
  inCodeBlock: boolean; // In a code block
  inQuote: boolean;     // In a blockquote
  currentHeading?: string; // Current section heading
  listType?: 'bullet' | 'numbered' | 'task'; // Type of list
}

interface UserPatterns {
  // Personal transition words the user often uses
  transitions: Map<string, number>; // "however" -> frequency
  // Common opening phrases
  openings: Map<string, number>;
  // Common closing phrases
  closings: Map<string, number>;
  // List continuation patterns
  listPatterns: Map<string, number>;
}

class NGramAutocompleteService {
  private unigram: NGramEntry = {};
  private bigram: Map<string, NGramEntry> = new Map();
  private trigram: Map<string, Map<string, number>> = new Map();
  private initialized = false;

  // User-specific patterns (learned over time)
  private userPatterns: UserPatterns = {
    transitions: new Map(),
    openings: new Map(),
    closings: new Map(),
    listPatterns: new Map(),
  };

  // Context window size - expanded from ~100 to 500 chars
  private readonly CONTEXT_WINDOW = 500;

  /**
   * Get autocomplete suggestions using n-gram patterns + enhanced context
   */
  async complete(
    textBefore: string,
    maxWords: number = 8,
    structure?: DocumentStructure,
    _semanticContext?: string
  ): Promise<string | null> {
    if (!this.initialized) {
      this.initializeDefaults();
    }

    // Use larger context window
    const contextText = textBefore.slice(-this.CONTEXT_WINDOW);

    // Extract words from context
    const words = this.extractWords(contextText);

    // Learn from this text (incremental)
    if (words.length > 0) {
      this.learnFromWords(words);
    }

    console.log('[NGramAutocomplete] Context:', {
      words: words.length,
      lastWords: words.slice(-3),
      structure,
    });

    if (words.length === 0) {
      return this.getCommonCompletion(textBefore, maxWords);
    }

    const last1 = words[words.length - 1];
    const last2 = words.length >= 2 ? `${words[words.length - 2]} ${last1}` : null;

    // Try trigram first (most specific) - returns 3rd word after 2-word prefix
    if (last2 && this.trigram.has(last2)) {
      const entry = this.trigram.get(last2)!;
      const trigramSuggestions = Object.entries(entry)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(e => e[0]);
      // Trigram gives us the 3rd word, so combine with prefix for 2-word completion
      if (trigramSuggestions.length > 0) {
        return `${last2} ${trigramSuggestions[0]}`;
      }
    }

    // Fall back to bigram - but return both words (prefix + suggestion)
    if (this.bigram.has(last1)) {
      const entry = this.bigram.get(last1)!;
      const bigramSuggestions = Object.entries(entry)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 5)
        .map(e => e[0]);

      // Filter and get best suggestion
      const recentWords = new Set(words.slice(-5));
      const validSuggestions = bigramSuggestions.filter(w => !recentWords.has(w) && w !== last1);

      if (validSuggestions.length > 0) {
        // Return both words: last word + predicted next word
        return `${last1} ${validSuggestions[0]}`;
      }
    }

    // No direct pattern match - try completions dictionary with 2-word phrases
    if (last1) {
      const twoWordCompletions = this.getTwoWordCompletion(last1, words, textBefore);
      if (twoWordCompletions) {
        return twoWordCompletions;
      }
    }

    // Try document-structure-specific suggestions (returns 2-word)
    if (structure) {
      const structuralSuggestion = this.getStructuralSuggestion(words, structure);
      if (structuralSuggestion) {
        return `${last1} ${structuralSuggestion}`;
      }
    }

    // Try user pattern suggestions (returns 2-word)
    const patternSuggestion = this.getPatternSuggestion(words, textBefore);
    if (patternSuggestion) {
      return `${last1} ${patternSuggestion}`;
    }

    // Final fallback - return common 2-word completion
    return this.getCommonCompletion(textBefore, 2);
  }

  /**
   * Get 2-word completion from dictionary
   */
  private getTwoWordCompletion(lastWord: string, _allWords: string[], textBefore: string): string | null {
    const recentText = textBefore.toLowerCase().slice(-100);

    // Common 2-word completions
    const twoWordPhrases: Record<string, string[]> = {
      'the': ['same time', 'first step', 'next step', 'most important', 'main goal', 'key point'],
      'and': ['then we', 'also the', 'so we', 'in addition', 'as well', 'finally we'],
      'but': ['also we', 'instead we', 'rather we', 'still the', 'yet the'],
      'this': ['is the', 'will be', 'has been', 'means we', 'approach is', 'method is'],
      'that': ['is the', 'will be', 'means we', 'can be', 'should be'],
      'is': ['also the', 'still the', 'very much', 'quite good', 'really the'],
      'was': ['also the', 'still the', 'very much', 'quite good', 'actually the'],
      'a': ['very good', 'quite good', 'simple way', 'clear case', 'good example'],
      'an': ['important step', 'excellent example', 'interesting approach'],
      'to': ['do this', 'be done', 'get the', 'make sure', 'see the', 'use the'],
      'in': ['the end', 'this case', 'fact we', 'order to', 'addition we'],
      'for': ['the first', 'this purpose', 'example we', 'most cases'],
      'of': ['the most', 'course we', 'all the', 'this type', 'these cases'],
      'with': ['this approach', 'the help', 'a simple', 'the new'],
    };

    if (twoWordPhrases[lastWord]) {
      for (const phrase of twoWordPhrases[lastWord]) {
        if (!recentText.includes(phrase)) {
          console.log('[NGramAutocomplete] 2-word completion:', phrase);
          return phrase;
        }
      }
    }

    return null;
  }

  /**
   * Get suggestions based on document structure
   */

  /**
   * Get suggestions based on document structure
   */
  private getStructuralSuggestion(words: string[], structure: DocumentStructure): string | null {
    if (structure.inList) {
      // List continuation patterns
      if (structure.listType === 'bullet') {
        const listStarters = ['another', 'also', 'additionally', 'next', 'then'];
        for (const starter of listStarters) {
          if (!words.slice(-3).includes(starter)) {
            return starter;
          }
        }
      }

      // Task list patterns
      if (structure.listType === 'task') {
        const taskWords = ['done', 'complete', 'finish', 'start', 'begin', 'create', 'add'];
        for (const word of taskWords) {
          if (!words.slice(-5).includes(word)) {
            return word;
          }
        }
      }
    }

    if (structure.inCodeBlock) {
      // Code completion patterns
      const codeWords = ['return', 'function', 'const', 'let', 'if', 'for', 'while', 'await'];
      for (const word of codeWords) {
        if (!words.slice(-5).includes(word)) {
          return word;
        }
      }
    }

    return null;
  }

  /**
   * Get suggestions based on learned user patterns
   */
  private getPatternSuggestion(_words: string[], textBefore: string): string | null {
    const recentText = textBefore.slice(-200).toLowerCase();

    // Check for transition patterns
    for (const [transition, count] of this.userPatterns.transitions) {
      if (count > 2 && !recentText.includes(transition)) {
        // Prioritize high-frequency transitions
        return transition;
      }
    }

    return null;
  }

  /**
   * Extract words from text, handling various formats
   */
  private extractWords(text: string): string[] {
    return text.toLowerCase()
      .replace(/[^\w\s']/g, ' ')  // Keep apostrophes for contractions
      .split(/\s+/)
      .filter(w => w.length > 1);
  }

  /**
   * Learn from the user's writing (incremental)
   */
  private learnFromWords(words: string[]) {
    // Only update if we have new words
    if (words.length <= (this.unigram['total_words'] || 0)) {
      return;
    }

    // Update unigrams
    for (const word of words) {
      this.unigram[word] = (this.unigram[word] || 0) + 1;
    }

    // Update bigrams
    for (let i = 0; i < words.length - 1; i++) {
      const prefix = words[i];
      const next = words[i + 1];
      if (!this.bigram.has(prefix)) {
        this.bigram.set(prefix, {});
      }
      const entry = this.bigram.get(prefix)!;
      entry[next] = (entry[next] || 0) + 1;
    }

    // Update trigrams
    for (let i = 0; i < words.length - 2; i++) {
      const prefix = `${words[i]} ${words[i + 1]}`;
      const next = words[i + 2];
      if (!this.trigram.has(prefix)) {
        this.trigram.set(prefix, new Map());
      }
      const entry = this.trigram.get(prefix)!;
      entry.set(next, (entry.get(next) || 0) + 1);
    }

    // Learn user patterns
    this.learnUserPatterns(words);

    // Track total words
    this.unigram['total_words'] = words.length;
  }

  /**
   * Learn user-specific patterns from their writing
   */
  private learnUserPatterns(words: string[]) {
    const transitions = ['however', 'therefore', 'furthermore', 'moreover', 'consequently',
                        'additionally', 'meanwhile', 'nonetheless', 'accordingly', 'hence'];

    for (const word of words) {
      // Track transition words
      if (transitions.includes(word)) {
        this.userPatterns.transitions.set(
          word,
          (this.userPatterns.transitions.get(word) || 0) + 1
        );
      }
    }
  }

  /**
   * Get common completion words based on context
   * Now returns 2-word phrases for better predictions
   */
  private getCommonCompletion(textBefore: string, _maxWords: number): string | null {
    const words = textBefore.split(/\s+/);
    const lastWord = words[words.length - 1]?.toLowerCase();
    const recentText = textBefore.toLowerCase().slice(-100);

    // 2-word completions for better predictions
    const completions: Record<string, string[]> = {
      'the': ['same time', 'first step', 'next step', 'most important', 'main goal', 'key point', 'only way'],
      'and': ['then we', 'also the', 'so we', 'in addition', 'as well', 'finally we', 'now we'],
      'but': ['also we', 'instead we', 'rather we', 'still the', 'yet the', 'however we'],
      'this': ['is the', 'will be', 'has been', 'means we', 'approach is', 'method is', 'case is'],
      'that': ['is the', 'will be', 'means we', 'can be', 'should be', 'would be', 'might be'],
      'is': ['also the', 'still the', 'very much', 'quite good', 'really the', 'usually the'],
      'was': ['also the', 'still the', 'very much', 'quite good', 'actually the', 'originally the'],
      'a': ['very good', 'quite good', 'simple way', 'clear case', 'good example', 'great way'],
      'an': ['important step', 'excellent example', 'interesting approach', 'effective method'],
      'to': ['do this', 'be done', 'get the', 'make sure', 'see the', 'use the', 'ensure the'],
      'in': ['the end', 'this case', 'fact we', 'order to', 'addition we', 'general the'],
      'for': ['the first', 'this purpose', 'example we', 'most cases', 'this reason', 'the most'],
      'of': ['the most', 'course we', 'all the', 'this type', 'these cases', 'each of'],
      'with': ['this approach', 'the help', 'a simple', 'the new', 'this method'],
      'on': ['the other', 'a regular', 'this basis', 'the top', 'order to'],
      'at': ['the end', 'first glance', 'this point', 'the same', 'least once'],
    };

    // Check for exact word match - return 2-word phrase
    if (lastWord && completions[lastWord]) {
      for (const phrase of completions[lastWord]) {
        if (!recentText.includes(phrase)) {
          console.log('[NGramAutocomplete] 2-word completion for', lastWord, '->', phrase);
          return phrase;
        }
      }
    }

    // Check for partial word match
    if (lastWord) {
      for (const [prefix, options] of Object.entries(completions)) {
        if (lastWord.startsWith(prefix)) {
          for (const phrase of options) {
            if (!recentText.includes(phrase)) {
              console.log('[NGramAutocomplete] Partial 2-word completion for', lastWord, '->', phrase);
              return phrase;
            }
          }
        }
      }
    }

    // Generic fallbacks - return 2-word phrases
    const fallbacks = ['and then', 'or also', 'but also', 'so we', 'now we', 'this is'];

    for (const fallback of fallbacks) {
      const words = fallback.split(' ');
      if (words[0] !== lastWord && !recentText.includes(fallback)) {
        if (!lastWord || !fallback.startsWith(lastWord)) {
          console.log('[NGramAutocomplete] Fallback 2-word:', fallback);
          return fallback;
        }
      }
    }

    console.log('[NGramAutocomplete] No suitable completion found');
    return null;
  }

  /**
   * Initialize with default common patterns
   */
  private initializeDefaults() {
    // Enhanced default text with more diverse patterns
    const sampleText = `
      the first thing to do is to make sure that everything is in order and then we can proceed
      the next step is to check the results and then we can move on to the final stage
      the final stage is to review all the work and then we can submit it for approval
      however we need to consider that this might not be the best approach
      therefore we should look at other options that are available
      furthermore we must take into account the various factors that influence the outcome
      moreover the system provides several ways to accomplish this task
      consequently the results show significant improvement in performance
      in addition to this we can also implement the following features
      on the other hand there are some limitations to consider
      in contrast to the previous approach this method offers better results
      for example we could use the following techniques to improve efficiency
      to illustrate this point consider the following case study
      in this case the system performs well under most conditions
      under these circumstances the best option is to proceed carefully
      based on the results we can conclude that this approach is effective
      according to the study the implementation was successful
      in order to understand this better we need to analyze the data
      the main goal is to create a system that works efficiently and effectively
      the purpose of this is to provide better suggestions for autocomplete
      this is a test and an example of how the system works
      what we need to do next is to ensure that everything is properly configured
      when the user types something the system should provide relevant suggestions
      after that we can test the functionality and see how it performs
    `;

    const words = this.extractWords(sampleText);
    this.learnFromWords(words);

    this.initialized = true;
    console.log('[NGramAutocomplete] Model initialized with', words.length, 'words');
  }

  /**
   * Update the model with new text (learn from user writing)
   */
  learnFrom(text: string) {
    const words = this.extractWords(text);
    this.learnFromWords(words);
  }

  /**
   * Get statistics about learned patterns
   */
  getPatterns() {
    return {
      transitions: Object.fromEntries(this.userPatterns.transitions),
      unigramCount: Object.keys(this.unigram).length,
      bigramCount: this.bigram.size,
      trigramCount: this.trigram.size,
    };
  }
}

// Singleton instance
export const nGramAutocomplete = new NGramAutocompleteService();
