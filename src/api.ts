import { invoke } from "@tauri-apps/api/tauri";
import type { AppView, SettingsPayload, Snapshot } from "./types";

export async function bootstrap(): Promise<Snapshot> {
  return invoke("bootstrap");
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

export async function setActiveView(view: AppView): Promise<Snapshot> {
  return invoke("set_active_view", { view });
}

export async function resetAppData(): Promise<Snapshot> {
  return invoke("reset_app_data");
}
