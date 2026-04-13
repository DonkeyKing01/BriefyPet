export type PetStatus = "loading" | "needs-config" | "scanning" | "idle" | "new-info";

export type AppView = "reading" | "settings";

export type FitLevel = "high" | "medium" | "low";

export type Discipline =
  | "technology"
  | "humanities"
  | "news"
  | "social-science"
  | "science"
  | "medicine"
  | "life"
  | "other";

export type SourceKind =
  | "academic-journal"
  | "official-announcement"
  | "technical-blog"
  | "community-hotspot";

export type SourceModule =
  | "technology"
  | "social_science"
  | "business"
  | "growth"
  | "news_opinion"
  | "entertainment"
  | "science"
  | "medicine"
  | "other";

export type SourceBucket =
  | "research"
  | "academic_frontier"
  | "official"
  | "blogs"
  | "community"
  | "streaming"
  | "news"
  | "personal_opinion"
  | "streaming_opinion"
  | "community_opinion"
  | "media_opinion"
  | "lite_pool"
  | "physics"
  | "chemistry"
  | "biology"
  | "unspecified";

export type ResourceType = "article" | "podcast" | "video" | "twitter" | "other";

export type UserDisciplinePreference = {
  discipline: Discipline;
  enabled: boolean;
  preference: string;
};

export type RssSource = {
  id: string;
  name: string;
  url: string;
  module: SourceModule;
  bucket: SourceBucket;
  discipline: Discipline;
  sourceKind: SourceKind;
  resourceType: ResourceType;
  language: string | null;
  enabled: boolean;
  enabledByDefault: boolean;
  postponed: boolean;
  originFiles: string[];
};

export type SettingsPayload = {
  apiKey: string;
  autoStart: boolean;
  disciplines: UserDisciplinePreference[];
  memoryModeEnabled: boolean;
  memorySummary: string;
  rssSources: RssSource[];
};

export type Article = {
  id: number;
  sourceId: string;
  title: string;
  link: string;
  sourceName: string;
  discipline: Discipline;
  sourceKind: SourceKind;
  resourceType: ResourceType;
  publishedAt: string | null;
  fetchedAt: string | null;
  summary: string;
  fitLevel: FitLevel;
  fitScore: number;
  recommendationReason: string;
  rawContent: string;
  isFavorite: boolean;
  isNew: boolean;
};

export type ReminderBatch = {
  id: string;
  articleIds: number[];
  articleCount: number;
  topArticleId: number | null;
  partitionCount: number;
};

export type ContentPoolStat = {
  sourceKind: SourceKind;
  totalArticles: number;
  candidateCount: number;
  topScore: number | null;
};

export type InterestMemoryRecord = {
  dayKey: string;
  generatedSummary: string;
  summary: string;
  memoryModeEnabled: boolean;
  eventCount: number;
  updatedAt: string | null;
};

export type SourceCatalogSummary = {
  totalSources: number;
  enabledSources: number;
  postponedSources: number;
  selectedDisciplines: number;
  dueSources: number;
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
  contentPoolStats: ContentPoolStat[];
  memory: InterestMemoryRecord | null;
  sourceSummary: SourceCatalogSummary;
};
