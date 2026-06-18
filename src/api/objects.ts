// Semantic Object API bindings for MarlOS
// These map to the Tauri commands in commands.rs

import { invoke } from "@tauri-apps/api/core";

// Types matching Rust structs

export type SecurityTier = "Open" | "Guarded" | "Sealed";

export type ContentType =
  | "text"
  | "markdown"
  | "json"
  | "code"
  | "binary"
  | "reference";

export interface SemanticObject {
  suid: string;
  created_at: string;
  modified_at: string;
  version: number;
  content_type: ContentType;
  title?: string;
  content: string;
  metadata: Record<string, unknown>;
  tier: SecurityTier;
  origin?: string;
}

export interface ObjectSearchResult {
  suid: string;
  title?: string;
  content_type: ContentType;
  tier: SecurityTier;
  relevance: number;
}

export interface WaveformSimilarityComponents {
  cross_correlation: number;
  spectral: number;
  multiscale: number;
}

export interface WaveformSearchResult {
  suid: string;
  title?: string;
  content_type: ContentType;
  tier: SecurityTier;
  score: number;
  waveform: WaveformSimilarityComponents;
  saturation_detected: boolean;
}

export interface TierChange {
  from: SecurityTier;
  to: SecurityTier;
  changed_by: string;
  reason: string;
  timestamp: string;
}

// Object CRUD operations

export async function createObject(
  content: string,
  contentType: ContentType = "text",
  title?: string,
  metadata?: Record<string, unknown>
): Promise<SemanticObject> {
  return invoke("object_create", {
    content,
    contentType,
    title,
    metadata,
  });
}

export async function getObject(suid: string): Promise<SemanticObject | null> {
  return invoke("object_get", { suid });
}

export async function listObjects(
  contentType?: ContentType,
  tier?: SecurityTier,
  limit?: number
): Promise<SemanticObject[]> {
  return invoke("object_list", { contentType, tier, limit });
}

export async function deleteObject(suid: string): Promise<boolean> {
  return invoke("object_delete", { suid });
}

// Search operations

export async function searchObjects(
  query: string,
  limit?: number,
  tierFilter?: SecurityTier
): Promise<ObjectSearchResult[]> {
  return invoke("object_search", { query, limit, tierFilter });
}

export async function findSimilar(
  suid: string,
  limit?: number
): Promise<ObjectSearchResult[]> {
  return invoke("object_find_similar", { suid, limit });
}

// File operations

export async function importFile(path: string): Promise<SemanticObject> {
  return invoke("object_import_file", { path });
}

export async function exportFile(
  suid: string,
  path: string
): Promise<string> {
  return invoke("object_export_file", { suid, path });
}

// Tier management (LLM can use these)

export async function getTier(suid: string): Promise<SecurityTier | null> {
  return invoke("object_get_tier", { suid });
}

export async function setTier(
  suid: string,
  tier: SecurityTier,
  reason: string,
  changedBy: string = "user"
): Promise<boolean> {
  return invoke("object_set_tier", { suid, tier, reason, changedBy });
}

export async function getTierHistory(suid: string): Promise<TierChange[]> {
  return invoke("object_tier_history", { suid });
}

// Relations

export async function addRelation(
  fromSuid: string,
  toSuid: string,
  relationType: string
): Promise<boolean> {
  return invoke("object_add_relation", { fromSuid, toSuid, relationType });
}

// Waveform-based search (signal processing approach)

/**
 * Search objects using waveform similarity
 *
 * Treats embeddings as signals and uses cross-correlation, spectral analysis,
 * and multi-scale comparison to find similarities that cosine similarity misses.
 * Particularly effective at scale where cosine similarity saturates.
 */
export async function searchObjectsWaveform(
  query: string,
  limit?: number,
  tierFilter?: SecurityTier
): Promise<WaveformSearchResult[]> {
  const results = await invoke("object_search_waveform", {
    query,
    limit,
    max_tier: tierFilter,
  });

  // Map backend response to frontend types
  return (results as any[]).map((r) => ({
    suid: r.object.suid,
    title: r.object.name,
    content_type: r.object.content_type as ContentType,
    tier: r.object.security_tier as SecurityTier,
    score: r.score,
    waveform: {
      cross_correlation: r.waveform.cross_correlation,
      spectral: r.waveform.spectral,
      multiscale: r.waveform.multiscale,
    },
    saturation_detected: r.saturation_detected,
  }));
}

/**
 * Find objects similar to a given object using waveform similarity
 */
export async function findSimilarWaveform(
  suid: string,
  limit?: number
): Promise<WaveformSearchResult[]> {
  const results = await invoke("object_find_similar_waveform", {
    suid,
    limit,
  });

  // Map backend response to frontend types
  return (results as any[]).map((r) => ({
    suid: r.object.suid,
    title: r.object.name,
    content_type: r.object.content_type as ContentType,
    tier: r.object.security_tier as SecurityTier,
    score: r.score,
    waveform: {
      cross_correlation: r.waveform.cross_correlation,
      spectral: r.waveform.spectral,
      multiscale: r.waveform.multiscale,
    },
    saturation_detected: r.saturation_detected,
  }));
}

// Helper functions

export function tierColor(tier: SecurityTier): string {
  switch (tier) {
    case "Open": return "#4ade80";      // green
    case "Guarded": return "#facc15";   // yellow
    case "Sealed": return "#f87171";    // red
    default: return "#9ca3af";          // gray
  }
}

export function tierIcon(tier: SecurityTier): string {
  switch (tier) {
    case "Open": return "🔓";
    case "Guarded": return "🔒";
    case "Sealed": return "🔐";
    default: return "❓";
  }
}

export function contentTypeIcon(type: ContentType): string {
  switch (type) {
    case "text": return "📄";
    case "markdown": return "📝";
    case "json": return "{}";
    case "code": return "💻";
    case "binary": return "📦";
    case "reference": return "🔗";
    default: return "📄";
  }
}
