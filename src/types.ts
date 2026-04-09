export type PetStatus = "loading" | "needs-config" | "scanning" | "idle" | "new-info";

export type AppView = "reading" | "settings";

export type FitLevel = "high" | "medium" | "low";

export type RssSource = {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
};

export type SettingsPayload = {
  apiKey: string;
  interestProfile: string;
  autoStart: boolean;
  rssSources: RssSource[];
};

export type Article = {
  id: number;
  title: string;
  link: string;
  sourceName: string;
  publishedAt: string | null;
  fetchedAt: string | null;
  summary: string;
  fitLevel: FitLevel;
  fitScore: number;
  recommendationReason: string;
  isFavorite: boolean;
  isNew: boolean;
};

export type ReminderBatch = {
  id: string;
  articleIds: number[];
  articleCount: number;
  topArticleId: number | null;
};

export type Snapshot = {
  settings: SettingsPayload;
  petStatus: PetStatus;
  articles: Article[];
  activeReminder: ReminderBatch | null;
  selectedArticleId: number | null;
  activeView: AppView;
  lastError: string | null;
  apiKeyValid: boolean;
  lastScanAt: string | null;
};
