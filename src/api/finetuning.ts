/**
 * Fine-Tuning API
 *
 * Frontend API for exporting chat data and managing fine-tuning workflows.
 */

import { invoke } from "@tauri-apps/api/core";

export interface ExportFilters {
  content_types?: string[];
  security_tiers?: string[];
  start_date?: string;
  end_date?: string;
  min_message_count?: number;
  limit?: number;
}

export interface ExportStats {
  objects_processed: number;
  conversations_exported: number;
  total_messages: number;
  output_path: string;
  export_time: string;
}

export interface ExportProviderStats {
  total_objects: number;
  total_messages: number;
  conversations: number;
  by_provider: Record<string, number>;
}

/**
 * Export conversations for fine-tuning
 */
export async function exportConversations(
  filters?: ExportFilters,
  output_path?: string
): Promise<ExportStats> {
  const defaultPath = await get_default_export_path();
  const outputPath = output_path || defaultPath;

  return invoke<ExportStats>("export_conversations_for_finetuning", {
    filters,
    outputPath,
  });
}

/**
 * Get statistics about available export data
 */
export async function getExportStats(): Promise<ExportProviderStats> {
  return invoke<ExportProviderStats>("get_export_stats");
}

/**
 * Get default export path based on platform
 */
async function get_default_export_path(): Promise<string> {
  const path = await invoke<string>("get_app_data_dir");
  return `${path}/finetuning_export.jsonl`;
}
