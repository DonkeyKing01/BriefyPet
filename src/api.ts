import { invoke } from "@tauri-apps/api/tauri";
import type { AppView, HistoryItem, OverlaySnapshot, SettingsPayload, Snapshot } from "./types";

export async function bootstrap(): Promise<Snapshot> {
  return invoke("bootstrap");
}

export async function bootstrapOverlay(): Promise<OverlaySnapshot> {
  return invoke("bootstrap_overlay");
}

export async function saveSettings(settings: SettingsPayload): Promise<Snapshot> {
  return invoke("save_settings", { settings });
}

export async function openArticle(articleId: number): Promise<Snapshot> {
  return invoke("open_article", { articleId });
}

export async function toggleFavorite(articleId: number): Promise<Snapshot> {
  return invoke("toggle_favorite", { articleId });
}

export async function petDoubleClick(): Promise<void> {
  return invoke("pet_double_click");
}

export async function bubbleAction(action: "view" | "snooze" | "ignore"): Promise<Snapshot> {
  return invoke("bubble_action", { action });
}

export async function openHelpWindow(): Promise<void> {
  return invoke("open_help_window");
}

export async function dismissHelpWindow(completeOnboarding: boolean): Promise<void> {
  return invoke("dismiss_help_window", { completeOnboarding });
}

export async function submitMemoryReview(
  action: "accept" | "modify" | "reject",
  summary?: string,
): Promise<Snapshot> {
  return invoke("submit_memory_review", { action, summary });
}

export async function setActiveView(view: AppView): Promise<Snapshot> {
  return invoke("set_active_view", { view });
}

export async function saveArticleNote(articleId: number, note: string): Promise<Snapshot> {
  return invoke("save_article_note", { articleId, note });
}

export async function getArticleRawContent(articleId: number): Promise<string> {
  return invoke("get_article_raw_content", { articleId });
}

export async function listHistoryArticlesPage(
  offset: number,
  limit: number,
): Promise<HistoryItem[]> {
  return invoke("list_history_articles_page", { offset, limit });
}

export async function addCustomRssSource(
  name: string,
  url: string,
  module: string,
  bucket: string,
  group: string,
): Promise<Snapshot> {
  return invoke("add_custom_rss_source", { name, url, module, bucket, group });
}

export async function resetRuntimeData(): Promise<void> {
  return invoke("reset_runtime_data");
}
