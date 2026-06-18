/**
 * Semantic Autocomplete Service
 *
 * Uses MarlOS's semantic search to find contextually relevant content
 * for better autocomplete suggestions.
 */

import { invoke } from "@tauri-apps/api/core";

interface SemanticContext {
  similarText: string[];
  relatedTopics: string[];
  recentDocuments: string[];
}

class SemanticAutocompleteService {
  private cache: Map<string, SemanticContext> = new Map();
  private cacheExpiry: number = 60000; // 1 minute cache
  private lastCacheUpdate: number = 0;

  /**
   * Get semantic context for autocomplete
   * Returns similar content, related topics, and recent documents
   */
  async getSemanticContext(currentText: string, maxResults: number = 3): Promise<SemanticContext> {
    const cacheKey = currentText.slice(-100); // Cache based on recent text

    // Check cache
    const cached = this.cache.get(cacheKey);
    if (cached && Date.now() - this.lastCacheUpdate < this.cacheExpiry) {
      return cached;
    }

    try {
      // Search for semantically similar content
      const searchResults = await invoke('object_search', {
        query: currentText.slice(-200), // Search with recent context
        limit: maxResults,
        maxTier: "open",
      });

      // Extract relevant context from search results
      const similarText: string[] = [];
      const relatedTopics: string[] = [];

      for (const result of (searchResults as any[])) {
        if (result.content) {
          similarText.push(result.content);
        }
        if (result.tags) {
          relatedTopics.push(...result.tags.filter((t: string) =>
            t.startsWith('topic:') || t.startsWith('category:')
          ));
        }
      }

      // Get recent documents
      const recentDocs = await this.getRecentDocuments(maxResults);

      const context: SemanticContext = {
        similarText: similarText.slice(0, maxResults),
        relatedTopics: [...new Set(relatedTopics)].slice(0, 5),
        recentDocuments: recentDocs,
      };

      // Update cache
      this.cache.set(cacheKey, context);
      this.lastCacheUpdate = Date.now();

      return context;
    } catch (error) {
      console.warn('[SemanticAutocomplete] Search failed:', error);
      return {
        similarText: [],
        relatedTopics: [],
        recentDocuments: [],
      };
    }
  }

  /**
   * Get recently viewed documents
   */
  private async getRecentDocuments(limit: number = 5): Promise<string[]> {
    try {
      // Get recent objects from the object store
      const recent = await invoke('object_list', {
        limit,
        offset: 0,
      });

      return (recent as any[])
        .map((obj: any) => obj.name)
        .filter(Boolean)
        .slice(0, limit);
    } catch (error) {
      console.warn('[SemanticAutocomplete] Failed to get recent documents:', error);
      return [];
    }
  }

  /**
   * Extract common phrases from similar text
   * Useful for learning user's writing patterns
   */
  extractCommonPhrases(similarText: string[], currentText: string): string[] {
    const phrases: string[] = [];
    const currentWords = new Set(currentText.toLowerCase().split(/\s+/));

    for (const text of similarText) {
      // Extract sentences
      const sentences = text.split(/[.!?]+/);

      for (const sentence of sentences) {
        const words = sentence.trim().split(/\s+/);
        if (words.length >= 2 && words.length <= 5) {
          const phrase = words.join(' ');
          // Check if phrase contains words relevant to current context
          const hasRelevantWord = words.some(w => currentWords.has(w.toLowerCase()));
          if (hasRelevantWord) {
            phrases.push(phrase);
          }
        }
      }
    }

    // Return unique phrases, prioritized by length
    return [...new Set(phrases)]
      .sort((a, b) => b.length - a.length)
      .slice(0, 10);
  }

  /**
   * Clear the semantic cache
   */
  clearCache() {
    this.cache.clear();
    this.lastCacheUpdate = 0;
  }
}

// Singleton instance
export const semanticAutocomplete = new SemanticAutocompleteService();
