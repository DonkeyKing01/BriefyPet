import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "@tauri-apps/api/window";
import {
  addCustomRssSource,
  bootstrap,
  bootstrapOverlay,
  bubbleAction,
  getArticleRawContent,
  dismissHelpWindow,
  listHistoryArticlesPage,
  openHelpWindow,
  openArticle,
  petDoubleClick,
  resetRuntimeData,
  saveArticleNote,
  saveSettings,
  setActiveView,
  submitMemoryReview,
  toggleFavorite
} from "./api";
import type {
  AppView,
  Article,
  Discipline,
  FitLevel,
  HistoryItem,
  LlmProtocol,
  LlmProvider,
  MemoryReviewProposal,
  OverlaySnapshot,
  RssSource,
  SettingsPayload,
  Snapshot,
  SourceBucket,
  SourceKind,
  SourceModule,
  UserDisciplinePreference
} from "./types";

const PET_STATUS_LABELS = {
  loading: "加载中",
  "needs-config": "待配置",
  polling: "轮询中",
  scanning: "扫描中",
  idle: "待命中",
  "new-info": "新提醒"
} as const;

const PET_STATUS_HINTS = {
  loading: "",
  "needs-config": "先去完善配置",
  polling: "轮询中",
  scanning: "按学科与子分类抓取中",
  idle: "当前没有高优提醒",
  "new-info": "双击或点气泡查看"
} as const;

const PET_ASSET_BY_STATUS = {
  loading: {
    src: "/pets/clawd/clawd-typing.gif",
    alt: "Clawd loading",
    size: 164
  },
  "needs-config": {
    src: "/pets/clawd/clawd-mini-peek.gif",
    alt: "Clawd needs config",
    size: 154
  },
  polling: {
    src: "/pets/clawd/clawd-idle-reading.gif",
    alt: "Clawd polling",
    size: 154
  },
  scanning: {
    src: "/pets/clawd/clawd-thinking.gif",
    alt: "Clawd scanning",
    size: 164
  },
  idle: {
    src: "/pets/clawd/clawd-mini-idle.gif",
    alt: "Clawd idle",
    size: 154
  },
  "new-info": {
    src: "/pets/clawd/clawd-mini-alert.gif",
    alt: "Clawd new info",
    size: 160
  }
} as const;

const MODULE_LABELS: Record<SourceModule, string> = {
  technology: "科技",
  social_science: "社科",
  business: "商业",
  growth: "成长",
  news_opinion: "新闻观点",
  entertainment: "娱乐",
  science: "科学",
  medicine: "医学",
  other: "其他"
};

const BUCKET_LABELS: Record<SourceBucket, string> = {
  research: "研究",
  academic_frontier: "学术前沿",
  official: "官方",
  blogs: "博客",
  community: "社区",
  streaming: "流媒体",
  news: "新闻",
  personal_opinion: "个人观点",
  streaming_opinion: "流媒体观点",
  community_opinion: "社区观点",
  media_opinion: "媒体观点",
  lite_pool: "轻量池",
  physics: "物理",
  chemistry: "化学",
  biology: "生物",
  unspecified: "未分类"
};

const DISCIPLINE_LABELS: Record<Discipline, string> = {
  technology: "科技",
  humanities: "娱乐",
  news: "新闻观点",
  "social-science": "社科",
  science: "科学",
  medicine: "医学",
  life: "成长",
  other: "商业"
};

const SOURCE_KIND_LABELS: Record<SourceKind, string> = {
  "academic-journal": "学术杂志",
  "official-announcement": "官方公告",
  "technical-blog": "技术博客",
  "community-hotspot": "社区热点"
};

const RESOURCE_TYPE_LABELS = {
  article: "文章",
  podcast: "播客",
  video: "视频",
  twitter: "社交源",
  other: "其他"
} as const;

const MODULE_ORDER: SourceModule[] = [
  "technology",
  "social_science",
  "business",
  "growth",
  "news_opinion",
  "entertainment",
  "science",
  "medicine",
  "other"
];

const BUCKET_ORDER: SourceBucket[] = [
  "research",
  "academic_frontier",
  "official",
  "blogs",
  "community",
  "streaming",
  "news",
  "personal_opinion",
  "streaming_opinion",
  "community_opinion",
  "media_opinion",
  "lite_pool",
  "physics",
  "chemistry",
  "biology",
  "unspecified"
];

const DISCIPLINE_ORDER: Discipline[] = [
  "technology",
  "social-science",
  "other",
  "life",
  "news",
  "humanities",
  "science",
  "medicine"
];

const PROVIDER_OPTIONS: Array<{ value: LlmProvider; label: string }> = [
  { value: "deepseek", label: "DeepSeek" },
  { value: "qwen", label: "Qwen" },
  { value: "minimax", label: "MiniMax" },
  { value: "glm", label: "GLM" },
  { value: "kimi", label: "Kimi" },
  { value: "openai", label: "OpenAI" },
  { value: "gemini", label: "Gemini" },
  { value: "anthropic", label: "Anthropic" },
  { value: "custom", label: "自定义" }
];

type ProviderModelOption = {
  id: string;
  name: string;
};

type ProviderDefinition = {
  label: string;
  protocol: LlmProtocol;
  baseUrl: string;
  models: ProviderModelOption[];
  apiKeyHint: string;
};

const PROVIDER_DEFINITIONS: Record<string, ProviderDefinition> = {
  deepseek: {
    label: "DeepSeek",
    protocol: "openai-compatible",
    baseUrl: "https://api.deepseek.com",
    models: [
      { id: "deepseek-chat", name: "DeepSeek Chat" },
      { id: "deepseek-reasoner", name: "DeepSeek Reasoner" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  qwen: {
    label: "Qwen",
    protocol: "openai-compatible",
    baseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
    models: [
      { id: "qwen3.5-flash", name: "Qwen 3.5 Flash" },
      { id: "qwen3.5-plus", name: "Qwen 3.5 Plus" },
      { id: "qwen3-max", name: "Qwen 3 Max" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  minimax: {
    label: "MiniMax",
    protocol: "openai-compatible",
    baseUrl: "https://api.minimaxi.com/v1",
    models: [
      { id: "MiniMax-M2.5-highspeed", name: "MiniMax M2.5 Highspeed" },
      { id: "MiniMax-M2.5", name: "MiniMax M2.5" },
      { id: "MiniMax-M2.7-highspeed", name: "MiniMax M2.7 Highspeed" },
      { id: "MiniMax-M2.7", name: "MiniMax M2.7" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  glm: {
    label: "GLM",
    protocol: "openai-compatible",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    models: [
      { id: "glm-4.7-flashx", name: "GLM 4.7 FlashX" },
      { id: "glm-5-turbo", name: "GLM 5 Turbo" },
      { id: "glm-4.7", name: "GLM 4.7" },
      { id: "glm-5.1", name: "GLM 5.1" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  kimi: {
    label: "Kimi",
    protocol: "openai-compatible",
    baseUrl: "https://api.moonshot.cn/v1",
    models: [
      { id: "kimi-k2.5", name: "Kimi K2.5" },
      { id: "kimi-k2-thinking", name: "Kimi K2 Thinking" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  openai: {
    label: "OpenAI",
    protocol: "openai-compatible",
    baseUrl: "https://api.openai.com/v1",
    models: [
      { id: "gpt-5.4-nano", name: "GPT-5.4 Nano" },
      { id: "gpt-5.4-mini", name: "GPT-5.4 Mini" },
      { id: "gpt-5.4", name: "GPT-5.4" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  gemini: {
    label: "Gemini",
    protocol: "gemini-native",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    models: [
      { id: "gemini-2.5-flash-lite", name: "Gemini 2.5 Flash-Lite" },
      { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash" },
      { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  anthropic: {
    label: "Anthropic",
    protocol: "anthropic-native",
    baseUrl: "https://api.anthropic.com",
    models: [
      { id: "claude-3-5-haiku-latest", name: "Claude Haiku 3.5" },
      { id: "claude-sonnet-4-20250514", name: "Claude Sonnet 4" },
      { id: "claude-opus-4-1-20250805", name: "Claude Opus 4.1" }
    ],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  custom: {
    label: "自定义",
    protocol: "openai-compatible",
    baseUrl: "",
    models: [],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  },
  siliconflow: {
    label: "SiliconFlow",
    protocol: "openai-compatible",
    baseUrl: "https://api.siliconflow.cn/v1",
    models: [{ id: "Qwen/Qwen2.5-72B-Instruct", name: "Qwen 2.5 72B Instruct" }],
    apiKeyHint: "这里填写当前所选服务商的 API Key"
  }
};

const PROTOCOL_OPTIONS: Array<{ value: LlmProtocol; label: string }> = [
  { value: "openai-compatible", label: "OpenAI Compatible" },
  { value: "anthropic-native", label: "Anthropic Native" },
  { value: "gemini-native", label: "Gemini Native" }
];

const DISCIPLINE_PLACEHOLDERS: Record<Discipline, string> = {
  technology:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：AI Agent、编程工具、大模型产品更新；优先教程、测评和重要发布",
  "social-science":
    "写下你关注的主题、想看的内容类型和优先条件\n例如：社会心理、青年文化、科技与社会；优先研究解读、案例分析和关键观点",
  other:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：AI 创业、产品策略、行业趋势；优先深度分析、公司动态和市场变化",
  life:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：学习方法、时间管理、表达沟通；优先可执行建议、经验总结和高质量书单",
  news:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：全球科技新闻、热点事件评论、产业政策变化；优先背景解读和多角度观点总结",
  humanities:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：电影、动画、游戏和流行文化；优先高质量推荐、口碑评价和新作信息",
  science:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：AI、认知科学、物理和前沿研究；优先通俗解读、重要论文和新发现",
  medicine:
    "写下你关注的主题、想看的内容类型和优先条件\n例如：睡眠、营养、运动健康和心理健康；优先循证研究、科普解读和实用建议"
};

const MODULE_BUCKET_MAP: Record<SourceModule, SourceBucket[]> = {
  technology: ["research", "official", "blogs", "community"],
  social_science: ["academic_frontier", "blogs", "community"],
  business: ["blogs", "community", "streaming"],
  growth: ["blogs", "community", "streaming"],
  news_opinion: ["news", "personal_opinion", "streaming_opinion", "community_opinion", "media_opinion"],
  entertainment: ["lite_pool"],
  science: ["physics", "chemistry", "biology"],
  medicine: ["academic_frontier", "blogs", "community"],
  other: ["unspecified"]
};

const MODULE_CONFIG_ORDER = MODULE_ORDER;

const MODULE_OPTIONS = MODULE_ORDER.filter((module) => module !== "other");

function getProviderDefinition(provider: LlmProvider): ProviderDefinition {
  return PROVIDER_DEFINITIONS[provider] ?? PROVIDER_DEFINITIONS.deepseek;
}

function activeProviderApiKeyKey(provider: LlmProvider, customProviderName: string) {
  if (provider !== "custom") {
    return provider;
  }
  const name = customProviderName.trim();
  return name ? `custom:${name}` : "custom";
}

const DEMO_SOURCES: RssSource[] = [
  {
    id: "demo-social-frontier",
    name: "Social Science Frontier",
    url: "https://example.com/social-frontier.xml",
    module: "social_science",
    bucket: "academic_frontier",
    discipline: "social-science",
    sourceKind: "academic-journal",
    resourceType: "article",
    language: "zh",
    enabled: true,
    enabledByDefault: true,
    postponed: false,
    originFiles: ["demo"]
  },
  {
    id: "demo-apple-dev",
    name: "Apple Developer News",
    url: "https://developer.apple.com/news/rss/news.rss",
    module: "technology",
    bucket: "official",
    discipline: "technology",
    sourceKind: "official-announcement",
    resourceType: "article",
    language: "en",
    enabled: true,
    enabledByDefault: true,
    postponed: false,
    originFiles: ["demo"]
  },
  {
    id: "demo-stratechery",
    name: "Stratechery",
    url: "https://stratechery.com/feed/",
    module: "business",
    bucket: "blogs",
    discipline: "other",
    sourceKind: "technical-blog",
    resourceType: "article",
    language: "en",
    enabled: true,
    enabledByDefault: true,
    postponed: false,
    originFiles: ["demo"]
  },
  {
    id: "demo-hacker-news",
    name: "Hacker News",
    url: "https://hnrss.org/frontpage",
    module: "technology",
    bucket: "community",
    discipline: "technology",
    sourceKind: "community-hotspot",
    resourceType: "article",
    language: "en",
    enabled: true,
    enabledByDefault: true,
    postponed: false,
    originFiles: ["demo"]
  },
  {
    id: "demo-science",
    name: "ScienceDaily",
    url: "https://www.sciencedaily.com/rss/top/science.xml",
    module: "science",
    bucket: "biology",
    discipline: "science",
    sourceKind: "academic-journal",
    resourceType: "article",
    language: "en",
    enabled: true,
    enabledByDefault: true,
    postponed: false,
    originFiles: ["demo"]
  }
];

const DEMO_NOW = Date.now();

const DEMO_ARTICLES: Article[] = [
  {
    id: 9001,
    sourceId: "demo-apple-dev",
    title: "Xcode 与 SwiftUI 发布节奏观察：如何提前规划 2026 开发周期",
    link: "https://developer.apple.com/news/",
    sourceName: "Apple Developer News",
    discipline: "technology",
    sourceKind: "official-announcement",
    resourceType: "article",
    publishedAt: new Date(DEMO_NOW - 1000 * 60 * 35).toISOString(),
    fetchedAt: new Date(DEMO_NOW - 1000 * 60 * 28).toISOString(),
    summary:
      "官方节奏正在从年更转向滚动更新，建议将团队计划拆为 API 稳定层和实验层两条流水线，以减少版本切换成本。",
    fitLevel: "high",
    fitScore: 92,
    recommendationReason: "与你的产品迭代节奏高度相关，且具备可执行的工程规划价值。",
    rawContent:
      "Apple 的工具链发布正在进入更高频的小步快跑节奏。对产品团队而言，最重要的是把版本升级从一次性大项目改成可持续维护任务。建议把开发计划拆分为两层：稳定层保障线上体验，实验层吸收新 API 与性能能力。",
    note: "",
    isFavorite: true,
    isNew: true
  },
  {
    id: 9002,
    sourceId: "demo-stratechery",
    title: "AI 订阅产品的单位经济：定价、留存与内容密度的三角平衡",
    link: "https://stratechery.com/",
    sourceName: "Stratechery",
    discipline: "other",
    sourceKind: "technical-blog",
    resourceType: "article",
    publishedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 3).toISOString(),
    fetchedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 2).toISOString(),
    summary:
      "文章提出以有效阅读时长和可复用洞察作为核心指标，替代单纯 DAU 来评估内容产品质量。",
    fitLevel: "high",
    fitScore: 88,
    recommendationReason: "对你的信息产品定位和商业化节奏有直接参考意义。",
    rawContent:
      "订阅型信息产品的核心，不是把更多内容塞给用户，而是稳定提升高价值内容命中率。文章提出以有效阅读时长和可复用洞察作为核心指标，替代单纯 DAU 来评估内容产品质量。",
    note: "",
    isFavorite: false,
    isNew: true
  },
  {
    id: 9003,
    sourceId: "demo-hacker-news",
    title: "一线工程团队如何把 RSS 与向量检索组合成高信号雷达",
    link: "https://news.ycombinator.com/",
    sourceName: "Hacker News",
    discipline: "technology",
    sourceKind: "community-hotspot",
    resourceType: "article",
    publishedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 9).toISOString(),
    fetchedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 8).toISOString(),
    summary:
      "社区讨论聚焦在低成本抓取、去重和摘要策略，结论是规则层和模型层需要并行迭代。",
    fitLevel: "medium",
    fitScore: 78,
    recommendationReason: "有方法论价值，但实践细节仍需结合你的实际流量进行验证。",
    rawContent:
      "社区讨论显示，纯规则或纯模型都难以长期维持质量。更好的方案是规则层先做召回筛选，模型层做语义排序与解释。对于桌宠类信息雷达，提醒机制应优先强调低打扰和高确定性。",
    note: "",
    isFavorite: false,
    isNew: false
  },
  {
    id: 9005,
    sourceId: "demo-social-frontier",
    title: "政策传播与平台机制：社科前沿研究给信息产品的三个启示",
    link: "https://example.com/social-frontier",
    sourceName: "Social Science Frontier",
    discipline: "social-science",
    sourceKind: "academic-journal",
    resourceType: "article",
    publishedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 6).toISOString(),
    fetchedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 5).toISOString(),
    summary:
      "研究指出，平台推荐机制会塑造公共议题扩散路径，产品在做提醒策略时应加入‘观点多样性’与‘证据密度’双指标。",
    fitLevel: "high",
    fitScore: 90,
    recommendationReason: "如果你关注社科，这篇内容能直接指导信息筛选与提醒策略设计。",
    rawContent:
      "社科研究提示，信息传播效率与信息质量并不总是同向增长。平台机制会偏向更高互动内容，但这不代表它们具有更高认知价值。产品若追求稳定认知增益，应同时关注热点追踪与证据优先。",
    note: "",
    isFavorite: false,
    isNew: true
  },
  {
    id: 9004,
    sourceId: "demo-science",
    title: "生物信息领域的新公开数据集：对健康类推荐系统的启示",
    link: "https://www.sciencedaily.com/",
    sourceName: "ScienceDaily",
    discipline: "science",
    sourceKind: "academic-journal",
    resourceType: "article",
    publishedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 20).toISOString(),
    fetchedAt: new Date(DEMO_NOW - 1000 * 60 * 60 * 18).toISOString(),
    summary:
      "该数据集补足了长期追踪维度，适合用于训练慢变量趋势模型，并可降低冷启动噪声。",
    fitLevel: "medium",
    fitScore: 74,
    recommendationReason: "属于中长期价值内容，适合进入你的科学子分类长期池。",
    rawContent:
      "新的公开数据集在时间维度上更完整，这意味着可以更准确识别长期趋势而非短期噪声。在健康与科学内容推荐中，这种慢变量往往比短期热门更有价值。",
    note: "",
    isFavorite: false,
    isNew: true
  }
];

type FeedSection = "today" | "favorites" | "history";

type FeedSelection =
  | { kind: "unread" }
  | { kind: "bucket"; section: FeedSection; module: string; bucket: string };

type ArticleMeta = {
  module: string;
  bucket: string;
  sourceName: string;
  pushedAt: string | null;
};

type ResizeTarget = "sidebar" | "timeline";

type SymbolName =
  | "today"
  | "unread"
  | "star"
  | "folder"
  | "chevron"
  | "settings"
  | "add"
  | "mark"
  | "open"
  | "share"
  | "sidebar"
  | "timeline"
  | "reader";

const HISTORY_PAGE_SIZE = 200;
const SNAPSHOT_EVENT = "briefy://snapshot-updated";
const OVERLAY_EVENT = "briefy://overlay-updated";

function snapshotFingerprint(snapshot: Snapshot) {
  const reminderKey = snapshot.activeReminder
    ? `${snapshot.activeReminder.id}:${snapshot.activeReminder.articleCount}:${snapshot.activeReminder.partitionCount}`
    : "none";
  const articleEdge = snapshot.articles
    .slice(0, 24)
    .map((article) => `${article.id}:${article.isNew ? 1 : 0}:${article.isFavorite ? 1 : 0}:${article.fitScore}`)
    .join(",");
  const historyEdge = snapshot.historyArticles
    .slice(0, 24)
    .map((item) => `${item.id}:${item.batchId}:${item.fitScore}`)
    .join(",");
  const enabledDisciplineCount = snapshot.settings.disciplines.filter((item) => item.enabled).length;
  const memoryReviewKey = snapshot.memoryReview
    ? `${snapshot.memoryReview.id}:${snapshot.memoryReview.status}:${snapshot.memoryReview.createdAt}`
    : "none";

  return [
    snapshot.petStatus,
    snapshot.activeView,
    snapshot.selectedArticleId ?? "",
    snapshot.lastScanAt ?? "",
    snapshot.lastError ?? "",
    snapshot.apiKeyValid ? "1" : "0",
    snapshot.settings.rssSources.length,
    enabledDisciplineCount,
    reminderKey,
    snapshot.articles.length,
    snapshot.historyArticles.length,
    snapshot.memory?.updatedAt ?? "",
    memoryReviewKey,
    snapshot.sourceSummary.dueSources,
    articleEdge,
    historyEdge
  ].join("|");
}

function useSnapshotEvents(enabled: boolean) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const fingerprintRef = useRef<string>("");

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let active = true;
    let unlisten: (() => void) | null = null;

    const applySnapshot = (next: Snapshot) => {
      const nextFingerprint = snapshotFingerprint(next);
      if (nextFingerprint !== fingerprintRef.current) {
        fingerprintRef.current = nextFingerprint;
        setSnapshot(next);
      }
    };

    const setup = async () => {
      try {
        unlisten = await listen<Snapshot>(SNAPSHOT_EVENT, (event) => {
          if (!active) {
            return;
          }
          applySnapshot(event.payload);
          setError(null);
          setLoading(false);
        });

        const initial = await bootstrap();
        if (!active) {
          return;
        }
        applySnapshot(initial);
        setError(null);
      } catch (err) {
        if (!active) {
          return;
        }
        setError(err instanceof Error ? err.message : "加载失败");
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    };

    setLoading(true);
    void setup();

    return () => {
      active = false;
      if (unlisten) {
        unlisten();
      }
    };
  }, [enabled]);

  return { snapshot, setSnapshot, loading, error };
}

function useOverlayEvents(enabled: boolean) {
  const [snapshot, setSnapshot] = useState<OverlaySnapshot | null>(null);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let active = true;
    let unlisten: (() => void) | null = null;

    const setup = async () => {
      try {
        unlisten = await listen<OverlaySnapshot>(OVERLAY_EVENT, (event) => {
          if (!active) {
            return;
          }
          setSnapshot(event.payload);
        });

        const initial = await bootstrapOverlay();
        if (!active) {
          return;
        }
        setSnapshot(initial);
      } catch {
        // Keep previous overlay state when transient event/bootstrap errors happen.
      }
    };

    void setup();

    return () => {
      active = false;
      if (unlisten) {
        unlisten();
      }
    };
  }, [enabled]);

  return snapshot;
}

function formatTime(value: string | null) {
  if (!value) {
    return "尚未完成抓取";
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(value));
}

function formatArticleTime(value: string | null) {
  if (!value) {
    return "未知";
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(value));
}

function formatTimelineTime(value: string | null) {
  if (!value) {
    return "未知时间";
  }

  const date = new Date(value);
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();

  if (sameDay) {
    return new Intl.DateTimeFormat("zh-CN", {
      hour: "2-digit",
      minute: "2-digit"
    }).format(date);
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit"
  }).format(date);
}

function fitLabel(level: FitLevel) {
  if (level === "high") {
    return "高";
  }
  if (level === "medium") {
    return "中";
  }
  return "低";
}

function toSafeTimestamp(value: string | null | undefined) {
  if (!value) {
    return 0;
  }

  const timestamp = new Date(value).getTime();
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function isSameCalendarDay(anchor: Date, value: string | null | undefined) {
  if (!value) {
    return false;
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return false;
  }

  return (
    date.getFullYear() === anchor.getFullYear() &&
    date.getMonth() === anchor.getMonth() &&
    date.getDate() === anchor.getDate()
  );
}

function compareWithPresetOrder(left: string, right: string, order: readonly string[]) {
  const leftIndex = order.indexOf(left);
  const rightIndex = order.indexOf(right);
  const leftRank = leftIndex === -1 ? Number.MAX_SAFE_INTEGER : leftIndex;
  const rightRank = rightIndex === -1 ? Number.MAX_SAFE_INTEGER : rightIndex;

  if (leftRank !== rightRank) {
    return leftRank - rightRank;
  }

  return left.localeCompare(right);
}

function orderedTreeEntries(tree: Map<string, Map<string, number>>) {
  return [...tree.keys()]
    .sort((left, right) => compareWithPresetOrder(left, right, MODULE_ORDER))
    .map((module) => {
      const buckets = [...(tree.get(module)?.entries() ?? [])].sort(([left], [right]) =>
        compareWithPresetOrder(left, right, BUCKET_ORDER)
      );
      return { module, buckets };
    });
}

function articleFingerprint(article: Article) {
  return [
    article.sourceId,
    article.link.trim().toLowerCase(),
    article.title.trim().toLowerCase(),
    article.publishedAt ?? "",
    article.fetchedAt ?? ""
  ].join("|");
}

function dedupeArticles(items: Article[]) {
  const seenIds = new Set<number>();
  const seenFingerprints = new Set<string>();
  const result: Article[] = [];

  for (const item of items) {
    const fingerprint = articleFingerprint(item);
    if (seenIds.has(item.id) || seenFingerprints.has(fingerprint)) {
      continue;
    }

    seenIds.add(item.id);
    seenFingerprints.add(fingerprint);
    result.push(item);
  }

  return result;
}

function dedupeHistoryItems(items: HistoryItem[]) {
  const sorted = [...items].sort(
    (left, right) => toSafeTimestamp(right.batchCreatedAt) - toSafeTimestamp(left.batchCreatedAt)
  );
  const seenIds = new Set<number>();
  const seenFingerprints = new Set<string>();
  const result: HistoryItem[] = [];

  for (const item of sorted) {
    const fingerprint = [
      item.sourceId,
      item.link.trim().toLowerCase(),
      item.title.trim().toLowerCase(),
      item.publishedAt ?? ""
    ].join("|");
    if (seenIds.has(item.id) || seenFingerprints.has(fingerprint)) {
      continue;
    }

    seenIds.add(item.id);
    seenFingerprints.add(fingerprint);
    result.push(item);
  }

  return result;
}

function articleTimestamp(article: Article, fallbackTime?: string | null) {
  return toSafeTimestamp(article.publishedAt ?? article.fetchedAt ?? fallbackTime ?? null);
}

function historyItemToArticle(item: HistoryItem): Article {
  return {
    id: item.id,
    sourceId: item.sourceId,
    title: item.title,
    link: item.link,
    sourceName: item.sourceName,
    discipline: "other",
    sourceKind: "technical-blog",
    resourceType: "article",
    publishedAt: item.publishedAt,
    fetchedAt: item.batchCreatedAt,
    summary: item.summary,
    fitLevel: item.fitLevel,
    fitScore: item.fitScore,
    recommendationReason: item.recommendationReason,
    rawContent: "",
    note: item.note,
    isFavorite: item.isFavorite,
    isNew: false
  };
}

function resolveArticleMeta(
  article: Article,
  sourceById: Map<string, RssSource>,
  historyByArticleId: Map<number, HistoryItem>
): ArticleMeta {
  const source = sourceById.get(article.sourceId);
  if (source) {
    return {
      module: source.module,
      bucket: source.bucket,
      sourceName: source.name,
      pushedAt: historyByArticleId.get(article.id)?.batchCreatedAt ?? null
    };
  }

  const history = historyByArticleId.get(article.id);
  if (history) {
    return {
      module: history.module || "other",
      bucket: history.bucket || "unspecified",
      sourceName: history.sourceName,
      pushedAt: history.batchCreatedAt ?? null
    };
  }

  return {
    module: "other",
    bucket: "unspecified",
    sourceName: article.sourceName,
    pushedAt: null
  };
}

function sortDisciplinePrefs(items: UserDisciplinePreference[]) {
  return [...items].sort(
    (left, right) =>
      DISCIPLINE_ORDER.indexOf(left.discipline) - DISCIPLINE_ORDER.indexOf(right.discipline)
  );
}

function groupSources(sources: RssSource[]) {
  const grouped = new Map<SourceModule, Map<SourceBucket, RssSource[]>>();
  for (const source of sources) {
    if (!grouped.has(source.module)) {
      grouped.set(source.module, new Map());
    }
    const moduleGroup = grouped.get(source.module)!;
    if (!moduleGroup.has(source.bucket)) {
      moduleGroup.set(source.bucket, []);
    }
    moduleGroup.get(source.bucket)!.push(source);
  }

  for (const module of grouped.keys()) {
    const bucketMap = grouped.get(module)!;
    const sortedBuckets = new Map<SourceBucket, RssSource[]>();
    for (const bucket of BUCKET_ORDER) {
      if (bucketMap.has(bucket)) {
        const list = [...bucketMap.get(bucket)!].sort((a, b) => a.name.localeCompare(b.name));
        sortedBuckets.set(bucket, list);
      }
    }
    grouped.set(module, sortedBuckets);
  }

  return grouped;
}

function moduleToDiscipline(module: SourceModule): Discipline {
  switch (module) {
    case "technology":
      return "technology";
    case "social_science":
      return "social-science";
    case "growth":
      return "life";
    case "news_opinion":
      return "news";
    case "entertainment":
      return "humanities";
    case "science":
      return "science";
    case "medicine":
      return "medicine";
    case "business":
    case "other":
    default:
      return "other";
  }
}

function sourceMatchesDisciplines(source: RssSource, selected: Set<Discipline>) {
  return selected.has(source.discipline) || selected.has(moduleToDiscipline(source.module));
}

function rssRawToText(value: string) {
  return value
    .replace(/<\s*br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|li|h[1-6]|blockquote|section|article)>/gi, "\n")
    .replace(/<script[\s\S]*?<\/script>/gi, " ")
    .replace(/<style[\s\S]*?<\/style>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&#39;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/[ \t]+/g, " ")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function SymbolIcon({ name, className }: { name: SymbolName; className?: string }) {
  const common = {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const
  };

  switch (name) {
    case "today":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <rect x="3" y="4.5" width="14" height="12" rx="2.5" {...common} />
          <path d="M6.5 3.5v3M13.5 3.5v3M3 8.5h14" {...common} />
        </svg>
      );
    case "unread":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path d="M4 5.5h12v9H4z" {...common} />
          <path d="M4 7l6 4 6-4" {...common} />
        </svg>
      );
    case "star":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path
            d="M10 3.8l1.9 3.8 4.2.6-3 2.9.7 4.1L10 13.3l-3.8 1.9.7-4.1-3-2.9 4.2-.6z"
            {...common}
          />
        </svg>
      );
    case "folder":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path d="M2.8 6.5h5l1.6 1.8h7.8v7.2a1.5 1.5 0 01-1.5 1.5H4.3a1.5 1.5 0 01-1.5-1.5z" {...common} />
        </svg>
      );
    case "chevron":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path d="M7.5 5.5l5 4.5-5 4.5" {...common} />
        </svg>
      );
    case "settings":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path
            d="M10 6.5a3.5 3.5 0 100 7 3.5 3.5 0 000-7zm0-3l1 1.9 2.2.4 1.5-1.5 1.4 1.4-1.5 1.5.4 2.2 1.9 1-1.9 1 .4 2.2 1.5 1.5-1.4 1.4-1.5-1.5-2.2.4-1 1.9-1-1.9-2.2-.4-1.5 1.5-1.4-1.4 1.5-1.5-.4-2.2-1.9-1 1.9-1-.4-2.2-1.5-1.5 1.4-1.4 1.5 1.5 2.2-.4z"
            {...common}
          />
        </svg>
      );
    case "add":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path d="M10 4.2v11.6M4.2 10h11.6" {...common} />
        </svg>
      );
    case "mark":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path d="M3.5 5.5h9v9h-9z" {...common} />
          <path d="M7 8.4l1.8 1.8 4-4" {...common} />
        </svg>
      );
    case "open":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <path d="M11.5 4h4.5v4.5" {...common} />
          <path d="M9 11l7-7" {...common} />
          <path d="M15.5 10v5.2a1.3 1.3 0 01-1.3 1.3H4.8a1.3 1.3 0 01-1.3-1.3V5.8a1.3 1.3 0 011.3-1.3H10" {...common} />
        </svg>
      );
    case "share":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <circle cx="5" cy="10" r="1.8" {...common} />
          <circle cx="15" cy="5" r="1.8" {...common} />
          <circle cx="15" cy="15" r="1.8" {...common} />
          <path d="M6.7 9l6.6-3M6.7 11l6.6 3" {...common} />
        </svg>
      );
    case "sidebar":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <rect x="2.5" y="3.5" width="15" height="13" rx="2" {...common} />
          <path d="M7 3.5v13" {...common} />
        </svg>
      );
    case "timeline":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <rect x="2.5" y="3.5" width="15" height="13" rx="2" {...common} />
          <path d="M9.8 3.5v13M14.3 3.5v13" {...common} />
        </svg>
      );
    case "reader":
      return (
        <svg className={className} viewBox="0 0 20 20" aria-hidden="true">
          <rect x="2.5" y="3.5" width="15" height="13" rx="2" {...common} />
          <path d="M6 7h8M6 10h8M6 13h5" {...common} />
        </svg>
      );
    default:
      return null;
  }
}

function PetWindow({ snapshot }: { snapshot: OverlaySnapshot | null }) {
  const status = snapshot?.petStatus ?? "loading";
  const articleCount = snapshot?.activeReminder?.articleCount ?? 0;
  const asset = PET_ASSET_BY_STATUS[status];
  const hint = PET_STATUS_HINTS[status];
  const pointerState = useRef<{ x: number; y: number; dragging: boolean } | null>(null);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) {
      return;
    }
    pointerState.current = {
      x: event.clientX,
      y: event.clientY,
      dragging: false
    };
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const state = pointerState.current;
    if (!state || state.dragging) {
      return;
    }

    const movedX = Math.abs(event.clientX - state.x);
    const movedY = Math.abs(event.clientY - state.y);
    if (movedX + movedY < 4) {
      return;
    }

    state.dragging = true;
    void appWindow.startDragging();
  }

  function handlePointerEnd() {
    pointerState.current = null;
  }

  return (
    <div
      className={`pet-shell pet-${status}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerEnd}
      onPointerCancel={handlePointerEnd}
      onPointerLeave={handlePointerEnd}
      onDoubleClick={() => {
        void petDoubleClick();
      }}
    >
      <div className="pet-shadow" />
      <div className="pet-stage">
        <img
          className="pet-character-image"
          src={asset.src}
          alt={asset.alt}
          draggable={false}
          style={{ width: `${asset.size}px`, height: `${asset.size}px` }}
        />
        {status === "new-info" && <div className="pet-badge">{articleCount}</div>}
        {hint && <div className="pet-whisper">{hint}</div>}
      </div>
    </div>
  );
}

function BubbleWindow({ snapshot }: { snapshot: OverlaySnapshot | null }) {
  const reminder = snapshot?.activeReminder;
  const pointerState = useRef<{ x: number; y: number; dragging: boolean } | null>(null);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) {
      return;
    }

    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a, input, textarea, select")) {
      pointerState.current = null;
      return;
    }

    pointerState.current = {
      x: event.clientX,
      y: event.clientY,
      dragging: false
    };
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const state = pointerState.current;
    if (!state || state.dragging) {
      return;
    }

    const movedX = Math.abs(event.clientX - state.x);
    const movedY = Math.abs(event.clientY - state.y);
    if (movedX + movedY < 4) {
      return;
    }

    state.dragging = true;
    void appWindow.startDragging();
  }

  function handlePointerEnd() {
    pointerState.current = null;
  }

  if (!reminder) {
    return <div className="bubble-empty" />;
  }

  return (
    <div
      className="bubble-window"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerEnd}
      onPointerCancel={handlePointerEnd}
      onPointerLeave={handlePointerEnd}
    >
      <div className="bubble-card">
        <p className="bubble-kicker">Briefy-pet 提醒</p>
        <h2>你有 {reminder.articleCount} 条新内容</h2>
        <p className="bubble-copy">
          当前提醒跨 {reminder.partitionCount} 个分区。你可以立刻查看，或仅延后这一批。
        </p>
        <div className="bubble-actions">
          <button onClick={() => void bubbleAction("view")}>立即查看</button>
          <button className="ghost" onClick={() => void bubbleAction("snooze")}>
            稍后 30 分钟
          </button>
          <button className="ghost" onClick={() => void bubbleAction("ignore")}>
            忽略本次
          </button>
        </div>
      </div>
    </div>
  );
}

function HelpWindow() {
  const pointerState = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  const steps = [
    {
      title: "欢迎使用 BriefyPet",
      body: "BriefyPet 会常驻桌面，帮你从订阅内容中挑出更值得优先阅读的信息，减轻信息焦虑。"
    },
    {
      title: "它能帮你做什么",
      body: "你可以按兴趣选择内容来源。\nBriefyPet 会自动检查更新、提炼重点，并把更相关的内容推送到桌面。\n你也可以在应用内阅读、收藏、标记未读和查看历史。"
    },
    {
      title: "为什么要配置 API Key",
      body: "BriefyPet 需要调用模型能力来理解内容、生成摘要和推荐结果。\n未配置或校验失败时，应用将无法正常分析内容。"
    },
    {
      title: "为什么要填写兴趣偏好",
      body: "你的兴趣偏好会影响内容推荐结果。\n写得越具体，推送、摘要和排序通常越准确。"
    },
    {
      title: "首次抓取说明",
      body: "首次启动后，BriefyPet 会先进行一次历史内容抓取与分析。\n为了保证信息质量，耗时较长属于正常现象，请耐心等待。"
    },
    {
      title: "项目与联系",
      body:
        "项目源码：\nhttps://github.com/DonkeyKing01/BriefyPet\n开发者邮箱：\nQingyang Jin: jinqingyang01@sjtu.edu.cn\nYuecheng He: 24300680058@m.fudan.edu.cn\n欢迎提交 PR 和联系我们。"
    },
    {
      title: "之后如何再次查看",
      body: "可点击主界面右上角的“帮助”重新打开本页。"
    }
  ] as const;
  const [stepIndex, setStepIndex] = useState(0);
  const isLastStep = stepIndex === steps.length - 1;

  useEffect(() => {
    const unlistenPromise = listen("briefy://help-opened", () => {
      setStepIndex(0);
    });

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) {
      return;
    }

    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a")) {
      pointerState.current = null;
      return;
    }

    pointerState.current = {
      x: event.clientX,
      y: event.clientY,
      dragging: false
    };
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const state = pointerState.current;
    if (!state || state.dragging) {
      return;
    }

    const movedX = Math.abs(event.clientX - state.x);
    const movedY = Math.abs(event.clientY - state.y);
    if (movedX + movedY < 4) {
      return;
    }

    state.dragging = true;
    void appWindow.startDragging();
  }

  function handlePointerEnd() {
    pointerState.current = null;
  }

  async function handleClose(complete: boolean) {
    await dismissHelpWindow(complete);
  }

  return (
    <div className="help-window">
      <div className="help-card">
        <div
          className="help-card-top"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerEnd}
          onPointerCancel={handlePointerEnd}
          onPointerLeave={handlePointerEnd}
        >
          <span className="help-eyebrow">Guide</span>
          <button className="help-close" onClick={() => void handleClose(true)}>
            跳过引导
          </button>
        </div>
        <div className="help-progress">
          {steps.map((_, index) => (
            <span
              key={index}
              className={`help-progress-dot${index === stepIndex ? " active" : ""}`}
            />
          ))}
        </div>
        <div className="help-body">
          <h1>{steps[stepIndex].title}</h1>
          <p>{steps[stepIndex].body}</p>
        </div>
        <div className="help-actions">
          {stepIndex > 0 && (
            <button className="help-secondary" onClick={() => setStepIndex((prev) => prev - 1)}>
              上一步
            </button>
          )}
          {isLastStep ? (
            <button className="help-primary" onClick={() => void handleClose(true)}>
              完成
            </button>
          ) : (
            <button className="help-primary" onClick={() => setStepIndex((prev) => prev + 1)}>
              下一步
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function MemoryReviewWindow({ proposal }: { proposal: MemoryReviewProposal | null | undefined }) {
  const [draft, setDraft] = useState(proposal?.proposedSummary ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(proposal?.proposedSummary ?? "");
    setError(null);
  }, [proposal?.id, proposal?.proposedSummary]);

  async function handleSubmit(action: "accept" | "modify" | "reject") {
    if (!proposal) {
      return;
    }
    if (action === "modify" && !draft.trim()) {
      setError("修改后的兴趣记忆不能为空。");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await submitMemoryReview(action, action === "modify" ? draft.trim() : undefined);
    } catch (err) {
      setError(err instanceof Error ? err.message : "提交记忆确认失败");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="memory-review-window">
      <div className="memory-review-card">
        <div className="memory-review-head">
          <span className="help-eyebrow">Weekly Memory Review</span>
          <strong>本周兴趣记忆更新</strong>
        </div>
        {proposal ? (
          <>
            <p className="memory-review-copy">
              系统已根据你本周的收藏内容、笔记和原始兴趣描述，生成一条更细的兴趣记忆。此窗口不能直接关闭，请选择接受、修改或拒绝。
            </p>
            <div className="memory-review-panel">
              <h3>当前基线</h3>
              <p>{proposal.baseSummary || "暂无已确认兴趣记忆，将以你当前填写的兴趣偏好为主。"}</p>
            </div>
            <div className="memory-review-panel">
              <h3>候选更新</h3>
              <textarea
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                placeholder="在这里修改本周候选兴趣记忆"
              />
            </div>
            {error && <div className="error-banner">{error}</div>}
            <div className="memory-review-actions">
              <button
                className="ghost"
                disabled={saving}
                onClick={() => void handleSubmit("reject")}
              >
                拒绝
              </button>
              <button
                className="ghost"
                disabled={saving}
                onClick={() => void handleSubmit("modify")}
              >
                {saving ? "提交中..." : "修改后接受"}
              </button>
              <button disabled={saving} onClick={() => void handleSubmit("accept")}>
                {saving ? "提交中..." : "直接接受"}
              </button>
            </div>
          </>
        ) : (
          <div className="memory-review-panel">
            <p>当前没有待确认的周度兴趣记忆。</p>
          </div>
        )}
      </div>
    </div>
  );
}

function SettingsView({
  snapshot,
  forceDisciplineSelection,
  setSnapshot
}: {
  snapshot: Snapshot;
  forceDisciplineSelection: boolean;
  setSnapshot: React.Dispatch<React.SetStateAction<Snapshot | null>>;
}) {
  const [saving, setSaving] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [addingSource, setAddingSource] = useState(false);
  const [addSourceError, setAddSourceError] = useState<string | null>(null);
  const [customName, setCustomName] = useState("");
  const [customUrl, setCustomUrl] = useState("");
  const [customModule, setCustomModule] = useState<SourceModule>("technology");
  const [customBucket, setCustomBucket] = useState<SourceBucket>("blogs");
  const [llmProvider, setLlmProvider] = useState<LlmProvider>(snapshot.settings.llmProvider);
  const [llmProtocol, setLlmProtocol] = useState<LlmProtocol>(snapshot.settings.llmProtocol);
  const [llmBaseUrl, setLlmBaseUrl] = useState(snapshot.settings.llmBaseUrl);
  const [llmCustomProviderName, setLlmCustomProviderName] = useState(
    snapshot.settings.llmCustomProviderName
  );
  const [llmModel, setLlmModel] = useState(snapshot.settings.llmModel);
  const [llmModelName, setLlmModelName] = useState(snapshot.settings.llmModelName);
  const [providerApiKeyDrafts, setProviderApiKeyDrafts] = useState<Record<string, string>>(
    snapshot.settings.providerApiKeys
  );

  const disciplinePrefs = useMemo(
    () => sortDisciplinePrefs(snapshot.settings.disciplines),
    [snapshot.settings.disciplines]
  );

  const groupedSources = useMemo(
    () => groupSources(snapshot.settings.rssSources),
    [snapshot.settings.rssSources]
  );

  const sortedModules = MODULE_ORDER.filter((module) => groupedSources.has(module));
  const providerDefinition = getProviderDefinition(llmProvider);
  const availableModels = llmProvider === "custom" ? [] : providerDefinition.models;
  const activeApiKeyKey = activeProviderApiKeyKey(llmProvider, llmCustomProviderName);
  const activeApiKey = providerApiKeyDrafts[activeApiKeyKey] ?? "";
  const customBucketOptions = MODULE_BUCKET_MAP[customModule];

  useEffect(() => {
    setLlmProvider(snapshot.settings.llmProvider);
    setLlmProtocol(snapshot.settings.llmProtocol);
    setLlmBaseUrl(snapshot.settings.llmBaseUrl);
    setLlmCustomProviderName(snapshot.settings.llmCustomProviderName);
    setLlmModel(snapshot.settings.llmModel);
    setLlmModelName(snapshot.settings.llmModelName);
    setProviderApiKeyDrafts(snapshot.settings.providerApiKeys);
  }, [
    snapshot.settings.llmBaseUrl,
    snapshot.settings.llmCustomProviderName,
    snapshot.settings.llmModel,
    snapshot.settings.llmModelName,
    snapshot.settings.llmProtocol,
    snapshot.settings.llmProvider,
    snapshot.settings.providerApiKeys
  ]);

  useEffect(() => {
    if (customBucketOptions.includes(customBucket)) {
      return;
    }
    setCustomBucket(customBucketOptions[0]);
  }, [customBucket, customBucketOptions]);

  useEffect(() => {
    if (llmProvider === "custom" || availableModels.length === 0) {
      return;
    }
    if (availableModels.some((item) => item.id === llmModel)) {
      return;
    }
    setLlmModel(availableModels[0].id);
    setLlmModelName(availableModels[0].name);
  }, [availableModels, llmModel, llmProvider]);

  function updateActiveApiKey(value: string) {
    setProviderApiKeyDrafts((prev) => ({
      ...prev,
      [activeApiKeyKey]: value
    }));
  }

  function applyProviderDefaults(nextProvider: LlmProvider) {
    const nextDefinition = getProviderDefinition(nextProvider);
    if (nextProvider === "custom") {
      setLlmProtocol(snapshot.settings.llmProtocol || "openai-compatible");
      setLlmBaseUrl(snapshot.settings.llmBaseUrl);
      setLlmCustomProviderName(snapshot.settings.llmCustomProviderName);
      setLlmModel(snapshot.settings.llmModel);
      setLlmModelName(snapshot.settings.llmModelName);
      return;
    }

    const nextModel = nextDefinition.models[0];
    setLlmProtocol(nextDefinition.protocol);
    setLlmBaseUrl(nextDefinition.baseUrl);
    setLlmModel(nextModel?.id ?? "");
    setLlmModelName(nextModel?.name ?? "");
  }

  async function handleSave(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const formData = new FormData(event.currentTarget);
    const providerApiKeys = Object.fromEntries(
      Object.entries(providerApiKeyDrafts).map(([key, value]) => [key, value.trim()])
    );
    providerApiKeys[activeApiKeyKey] = activeApiKey.trim();

    const disciplines = disciplinePrefs.map((item) => ({
      discipline: item.discipline,
      enabled: formData.get(`discipline-enabled-${item.discipline}`) === "on",
      preference: String(formData.get(`discipline-pref-${item.discipline}`) ?? "")
    }));

    if (!disciplines.some((item) => item.enabled)) {
      setSubmitError("请至少选择 1 个感兴趣学科，才能进入三栏阅读模式。");
      return;
    }

    if (llmProvider === "custom") {
      if (!llmCustomProviderName.trim()) {
        setSubmitError("自定义服务请填写 Provider。");
        return;
      }
      if (!llmProtocol.trim()) {
        setSubmitError("自定义服务请选择 API 协议。");
        return;
      }
      if (!llmBaseUrl.trim()) {
        setSubmitError("自定义服务请填写 Base URL。");
        return;
      }
      if (!llmModel.trim()) {
        setSubmitError("自定义服务请填写 Model ID。");
        return;
      }
      if (!llmModelName.trim()) {
        setSubmitError("自定义服务请填写 Model Name。");
        return;
      }
      if (!activeApiKey.trim()) {
        setSubmitError("自定义服务请填写 API Key。");
        return;
      }
    }

    const rssSources = snapshot.settings.rssSources.map((source) => ({
      ...source,
      enabled: formData.get(`source-${source.id}`) === "on"
    }));

    const moduleFetchIntervals = Object.fromEntries(
      MODULE_ORDER.map((module) => {
        const raw = Number(formData.get(`module-fetch-${module}`) ?? 12);
        return [module, Number.isFinite(raw) ? Math.max(1, Math.min(168, raw)) : 12];
      })
    ) as Record<SourceModule, number>;

    const modulePushTopN = Object.fromEntries(
      MODULE_ORDER.map((module) => {
        const raw = Number(formData.get(`module-push-topn-${module}`) ?? 6);
        return [module, Number.isFinite(raw) ? Math.max(1, Math.min(24, raw)) : 6];
      })
    ) as Record<SourceModule, number>;

    const payload: SettingsPayload = {
      apiKey: activeApiKey.trim(),
      llmProvider,
      llmProtocol,
      llmBaseUrl: llmBaseUrl.trim(),
      llmCustomProviderName: llmCustomProviderName.trim(),
      llmModelName: llmModelName.trim(),
      llmModel: llmModel.trim(),
      providerApiKeys,
      moduleFetchIntervals,
      modulePushTopN,
      autoStart: formData.get("autoStart") === "on",
      disciplines,
      memoryModeEnabled: formData.get("memoryModeEnabled") === "on",
      memorySummary: snapshot.settings.memorySummary,
      rssSources
    };

    setSaving(true);
    setSubmitError(null);

    try {
      const next = await saveSettings(payload);
      setSnapshot(next);
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : "保存设置失败");
    } finally {
      setSaving(false);
    }
  }

  async function handleAddCustomSource() {
    if (!customName.trim() || !customUrl.trim()) {
      setAddSourceError("请先填写自定义 RSS 的名称和 URL。");
      return;
    }

    setAddingSource(true);
    setAddSourceError(null);
    try {
      const next = await addCustomRssSource(customName, customUrl, customModule, customBucket);
      setSnapshot(next);
      setCustomName("");
      setCustomUrl("");
      setCustomBucket(MODULE_BUCKET_MAP[customModule][0]);
    } catch (err) {
      setAddSourceError(err instanceof Error ? err.message : "新增 RSS 失败");
    } finally {
      setAddingSource(false);
    }
  }

  async function handleResetRuntime() {
    const confirmed = window.confirm(
      "将清空全部数据库与配置缓存（包含推送库、设置、抓取状态）并重启应用，确认继续吗？"
    );
    if (!confirmed) {
      return;
    }
    await resetRuntimeData();
  }

  return (
    <form className="settings-view" onSubmit={handleSave}>
      {submitError && <div className="error-banner">{submitError}</div>}
      {forceDisciplineSelection && (
        <div className="onboarding-hint">
          首次进入请先选择感兴趣学科。保存后，左侧订阅池将只展示这些学科的订阅源。
        </div>
      )}

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>启动门槛</h2>
          <p>先选服务商，再选模型；只有当前服务商的 API Key 会参与抓取后的分析与评分。</p>
        </div>
        <label>
          <span>模型服务商</span>
          <select
            name="llmProvider"
            value={llmProvider}
            onChange={(event) => {
              const nextProvider = event.target.value as LlmProvider;
              setLlmProvider(nextProvider);
              applyProviderDefaults(nextProvider);
            }}
          >
            {PROVIDER_OPTIONS.map((provider) => (
              <option key={provider.value} value={provider.value}>
                {provider.label}
              </option>
            ))}
          </select>
        </label>

        {llmProvider === "custom" ? (
          <div className="settings-stack">
            <div className="discipline-grid">
              <label>
                <span>Provider</span>
                <input
                  type="text"
                  value={llmCustomProviderName}
                  onChange={(event) => setLlmCustomProviderName(event.target.value)}
                  placeholder="例如：My Provider"
                />
              </label>
              <label>
                <span>API 协议</span>
                <select
                  value={llmProtocol}
                  onChange={(event) => setLlmProtocol(event.target.value as LlmProtocol)}
                >
                  {PROTOCOL_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <label>
              <span>Base URL</span>
              <input
                type="text"
                value={llmBaseUrl}
                onChange={(event) => setLlmBaseUrl(event.target.value)}
                placeholder="https://your-provider.example.com/v1"
              />
            </label>
            <div className="discipline-grid">
              <label>
                <span>Model ID</span>
                <input
                  type="text"
                  value={llmModel}
                  onChange={(event) => setLlmModel(event.target.value)}
                  placeholder="model_id"
                />
              </label>
              <label>
                <span>Model Name</span>
                <input
                  type="text"
                  value={llmModelName}
                  onChange={(event) => setLlmModelName(event.target.value)}
                  placeholder="model_name"
                />
              </label>
            </div>
          </div>
        ) : (
          <div className="settings-stack">
            <div className="discipline-grid">
              <label>
                <span>模型</span>
                <select
                  value={llmModel}
                  onChange={(event) => {
                    const nextModelId = event.target.value;
                    const nextModel = availableModels.find((item) => item.id === nextModelId);
                    setLlmModel(nextModelId);
                    setLlmModelName(nextModel?.name ?? nextModelId);
                  }}
                >
                  {availableModels.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>API 协议</span>
                <input type="text" value={providerDefinition.protocol} readOnly />
              </label>
            </div>
            <label>
              <span>Base URL</span>
              <input type="text" value={providerDefinition.baseUrl} readOnly />
            </label>
          </div>
        )}

        <label>
          <span>API Key</span>
          <input
            type="password"
            value={activeApiKey}
            onChange={(event) => updateActiveApiKey(event.target.value)}
            placeholder={providerDefinition.apiKeyHint}
          />
        </label>
        <div className="settings-hint">当前激活服务商：{llmProvider}</div>
        <div className="settings-hint">当前激活 Key：{activeApiKey.trim() ? "已设置" : "未设置"}</div>
        <div className="settings-hint">
          保存后会自动校验并开始抓取。评分并发固定为 20，失败项会标记失败且不重复打分。
        </div>
        <label className="checkbox-row">
          <input name="autoStart" type="checkbox" defaultChecked={snapshot.settings.autoStart} />
          <span>开机启动</span>
        </label>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>结构化兴趣</h2>
          <p>为每个启用学科填写偏好，系统会据此完成摘要和推送判定。</p>
        </div>
        <div className="discipline-grid">
          {disciplinePrefs.map((item) => (
            <div key={item.discipline} className="discipline-card">
              <label className="discipline-toggle">
                <input
                  name={`discipline-enabled-${item.discipline}`}
                  type="checkbox"
                  defaultChecked={item.enabled}
                />
                <span>{DISCIPLINE_LABELS[item.discipline]}</span>
              </label>
              <textarea
                name={`discipline-pref-${item.discipline}`}
                defaultValue={item.preference}
                placeholder={DISCIPLINE_PLACEHOLDERS[item.discipline]}
              />
            </div>
          ))}
        </div>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>周度兴趣记忆</h2>
          <p>系统会在每周五晚九点（北京时间）基于本周收藏与评论生成一条候选兴趣记忆，等待你接受、修改或拒绝。</p>
        </div>
        <label className="checkbox-row">
          <input
            name="memoryModeEnabled"
            type="checkbox"
            defaultChecked={snapshot.settings.memoryModeEnabled}
          />
          <span>启用每日兴趣记忆</span>
        </label>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>Module 抓取频率</h2>
          <p>默认科技 6 小时，其余 12 小时；单位为小时。调度与推送都按 module 维度执行。</p>
        </div>
        <div className="discipline-grid">
          {MODULE_CONFIG_ORDER.map((module) => (
            <label key={`fetch-${module}`}>
              <span>{MODULE_LABELS[module]} 抓取间隔（小时）</span>
              <input
                name={`module-fetch-${module}`}
                type="number"
                min={1}
                max={168}
                defaultValue={snapshot.settings.moduleFetchIntervals[module]}
              />
            </label>
          ))}
        </div>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>Module 推送入池篇数</h2>
          <p>每次抓取评分后，按 module 全局分数降序取 Top N 进入推送池，之后仍需满足 60 分阈值才会提醒。</p>
        </div>
        <div className="discipline-grid">
          {MODULE_CONFIG_ORDER.map((module) => (
            <label key={`push-${module}`}>
              <span>{MODULE_LABELS[module]} Top N</span>
              <input
                name={`module-push-topn-${module}`}
                type="number"
                min={1}
                max={24}
                defaultValue={snapshot.settings.modulePushTopN[module]}
              />
            </label>
          ))}
        </div>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>源池开关</h2>
          <p>按 module / bucket 折叠管理信源；取消勾选将清除该源历史。</p>
        </div>
        <div className="source-groups">
          {sortedModules.map((module) => (
            <details key={module} className="source-discipline-block">
              <summary className="source-discipline-head">
                <div className="source-summary-main">
                  <span className="source-chevron" aria-hidden="true">
                    ▸
                  </span>
                  <h3>{MODULE_LABELS[module]}</h3>
                </div>
                <span className="source-summary-meta">
                  {Array.from(groupedSources.get(module)?.values() ?? []).reduce(
                    (total, sources) => total + sources.length,
                    0
                  )}{" "}
                  个源
                </span>
              </summary>
              {Array.from(groupedSources.get(module)!.entries()).map(([bucket, sources]) => (
                <details key={`${module}-${bucket}`} className="source-kind-block">
                  <summary className="source-kind-head">
                    <div className="source-summary-main">
                      <span className="source-chevron" aria-hidden="true">
                        ▸
                      </span>
                      <strong>{BUCKET_LABELS[bucket]}</strong>
                    </div>
                    <span>
                      {SOURCE_KIND_LABELS[sources[0].sourceKind]} · {sources.length} 个
                    </span>
                  </summary>
                  <div className="rss-list">
                    {sources.map((source) => (
                      <label key={source.id} className="rss-item">
                        <input
                          name={`source-${source.id}`}
                          type="checkbox"
                          defaultChecked={source.enabled}
                        />
                        <div>
                          <strong>{source.name}</strong>
                          <span>
                            {RESOURCE_TYPE_LABELS[source.resourceType]} · {source.url}
                          </span>
                        </div>
                      </label>
                    ))}
                  </div>
                </details>
              ))}
            </details>
          ))}
        </div>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>自定义 RSS</h2>
          <p>新增自己的 RSS 源并指定 module / bucket。</p>
        </div>
        {addSourceError && <div className="error-banner">{addSourceError}</div>}
        <label>
          <span>名称</span>
          <input
            type="text"
            value={customName}
            onChange={(event) => setCustomName(event.target.value)}
            placeholder="例如：My Research Feed"
          />
        </label>
        <label>
          <span>RSS URL</span>
          <input
            type="text"
            value={customUrl}
            onChange={(event) => setCustomUrl(event.target.value)}
            placeholder="https://example.com/feed.xml"
          />
        </label>
        <div className="discipline-grid">
          <label>
            <span>Module</span>
            <select
              value={customModule}
              onChange={(event) => setCustomModule(event.target.value as SourceModule)}
            >
              {MODULE_OPTIONS.map((module) => (
                <option key={module} value={module}>
                  {MODULE_LABELS[module]}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Bucket</span>
            <select
              value={customBucket}
              onChange={(event) => setCustomBucket(event.target.value as SourceBucket)}
            >
              {customBucketOptions.map((bucket) => (
                <option key={bucket} value={bucket}>
                  {BUCKET_LABELS[bucket]}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="settings-actions">
          <button
            type="button"
            disabled={addingSource}
            onClick={() => void handleAddCustomSource()}
          >
            {addingSource ? "添加中..." : "添加自定义 RSS"}
          </button>
        </div>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>全量重置</h2>
          <p>清空全部数据库与配置缓存并重启应用，恢复为首次启动状态。</p>
        </div>
        <div className="settings-actions">
          <button type="button" className="ghost-danger" onClick={() => void handleResetRuntime()}>
            全量重置并重启
          </button>
        </div>
      </section>

      <div className="settings-actions">
        <button type="submit" disabled={saving}>
          {saving ? "保存中..." : "保存设置"}
        </button>
      </div>
    </form>
  );
}

function MainWindow({
  snapshot,
  setSnapshot,
  loading,
  error
}: {
  snapshot: Snapshot | null;
  setSnapshot: React.Dispatch<React.SetStateAction<Snapshot | null>>;
  loading: boolean;
  error: string | null;
}) {
  const [selection, setSelection] = useState<FeedSelection>({ kind: "unread" });
  // collapsedFolders: key = "section:module" e.g. "today:technology"
  const [collapsedFolders, setCollapsedFolders] = useState<Record<string, boolean>>({});
  // section-level collapse: "today" | "favorites" | "history"
  const [sectionCollapsed, setSectionCollapsed] = useState<Record<FeedSection, boolean>>({
    today: false,
    favorites: false,
    history: true
  });
  const [manualUnreadIds, setManualUnreadIds] = useState<number[]>([]);
  const [archivedUnreadAt, setArchivedUnreadAt] = useState<Record<number, string>>({});
  const [unreadClickCounts, setUnreadClickCounts] = useState<Record<number, number>>({});
  const [demoFavoriteIds, setDemoFavoriteIds] = useState<number[]>(
    DEMO_ARTICLES.filter((item) => item.isFavorite).map((item) => item.id)
  );
  const [localSelectedId, setLocalSelectedId] = useState<number | null>(null);
  const [noteDraft, setNoteDraft] = useState("");
  const [noteSaving, setNoteSaving] = useState(false);
  const [selectedRawContent, setSelectedRawContent] = useState("");
  const [selectedRawArticleId, setSelectedRawArticleId] = useState<number | null>(null);
  const [rawContentLoading, setRawContentLoading] = useState(false);
  const [historyArchive, setHistoryArchive] = useState<HistoryItem[]>([]);
  const [historyOffset, setHistoryOffset] = useState(0);
  const [historyHasMore, setHistoryHasMore] = useState(true);
  const [historyLoadingMore, setHistoryLoadingMore] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [timelineCollapsed, setTimelineCollapsed] = useState(false);
  const [leftWidth, setLeftWidth] = useState(252);
  const [middleWidth, setMiddleWidth] = useState(332);
  const [interactionMessage, setInteractionMessage] = useState<string | null>(null);
  const [isCompact, setIsCompact] = useState(false);

  const layoutRef = useRef<HTMLDivElement | null>(null);
  const readerScrollRef = useRef<HTMLDivElement | null>(null);
  const resizeTargetRef = useRef<ResizeTarget | null>(null);

  const interestedDisciplines = useMemo(() => {
    const selected = new Set<Discipline>();
    for (const item of snapshot?.settings.disciplines ?? []) {
      if (item.enabled) {
        selected.add(item.discipline);
      }
    }
    return selected;
  }, [snapshot?.settings.disciplines]);

  const hasInterestedDiscipline = interestedDisciplines.size > 0;

  const isConfigurationComplete = useMemo(() => {
    if (!snapshot) {
      return false;
    }

    const hasApiKey = snapshot.settings.apiKey.trim().length > 0;
    if (!hasApiKey || !snapshot.apiKeyValid) {
      return false;
    }

    const enabled = snapshot.settings.disciplines.filter((item) => item.enabled);
    if (enabled.length === 0) {
      return false;
    }

    return enabled.every((item) => item.preference.trim().length > 0);
  }, [snapshot]);

  const usingDemoData = false;

  const activeSources = useMemo(() => {
    const all = snapshot?.settings.rssSources ?? [];
    const enabled = all.filter((source) => source.enabled);
    const sourcePool = enabled.length > 0 ? enabled : usingDemoData ? DEMO_SOURCES : all;

    if (!hasInterestedDiscipline) {
      return [];
    }

    return sourcePool.filter((source) => sourceMatchesDisciplines(source, interestedDisciplines));
  }, [snapshot?.settings.rssSources, usingDemoData, hasInterestedDiscipline, interestedDisciplines]);

  const activeSourceIds = useMemo(() => new Set(activeSources.map((source) => source.id)), [activeSources]);
  const sourceById = useMemo(
    () => new Map(activeSources.map((source) => [source.id, source])),
    [activeSources]
  );

  const manualUnreadSet = useMemo(() => new Set(manualUnreadIds), [manualUnreadIds]);
  const archivedUnreadSet = useMemo(
    () => new Set(Object.keys(archivedUnreadAt).map((id) => Number(id))),
    [archivedUnreadAt]
  );

  const reminderArticleIds = useMemo(
    () => new Set<number>(snapshot?.activeReminder?.articleIds ?? []),
    [snapshot?.activeReminder]
  );

  const allArticles = useMemo(() => {
    const all = snapshot?.articles ?? [];
    if (!hasInterestedDiscipline) {
      return [];
    }

    return dedupeArticles(
      all.filter((a) => activeSourceIds.has(a.sourceId) || interestedDisciplines.has(a.discipline))
    );
  }, [snapshot?.articles, hasInterestedDiscipline, activeSourceIds, interestedDisciplines]);

  const articleById = useMemo(() => new Map(allArticles.map((article) => [article.id, article])), [allArticles]);

  const historyItems = useMemo(() => dedupeHistoryItems(historyArchive), [historyArchive]);

  const historyByArticleId = useMemo(() => {
    const map = new Map<number, HistoryItem>();
    for (const item of historyItems) {
      const previous = map.get(item.id);
      if (!previous || toSafeTimestamp(item.batchCreatedAt) > toSafeTimestamp(previous.batchCreatedAt)) {
        map.set(item.id, item);
      }
    }
    return map;
  }, [historyItems]);

  const articleMetaById = useMemo(() => {
    const map = new Map<number, ArticleMeta>();

    for (const article of allArticles) {
      map.set(article.id, resolveArticleMeta(article, sourceById, historyByArticleId));
    }

    for (const item of historyItems) {
      map.set(item.id, {
        module: item.module || "other",
        bucket: item.bucket || "unspecified",
        sourceName: item.sourceName,
        pushedAt: item.batchCreatedAt ?? null
      });
    }

    return map;
  }, [allArticles, sourceById, historyByArticleId, historyItems]);

  const reminderArticles = useMemo(
    () => dedupeArticles(allArticles.filter((article) => reminderArticleIds.has(article.id))),
    [allArticles, reminderArticleIds]
  );

  const unreadArticles = useMemo(() => {
    const pushedUnread = reminderArticles.filter(
      (article) => !archivedUnreadSet.has(article.id) && reminderArticleIds.has(article.id) && article.isNew
    );
    const manualUnread = allArticles.filter(
      (article) => !archivedUnreadSet.has(article.id) && manualUnreadSet.has(article.id)
    );
    return dedupeArticles([...pushedUnread, ...manualUnread]);
  }, [reminderArticles, reminderArticleIds, archivedUnreadSet, manualUnreadSet, allArticles]);

  const unreadIdSet = useMemo(() => new Set(unreadArticles.map((article) => article.id)), [unreadArticles]);

  const todayFromHistory = useMemo(() => {
    const now = new Date();
    return historyItems
      .filter((item) => isSameCalendarDay(now, item.batchCreatedAt))
      .map((item) => articleById.get(item.id) ?? historyItemToArticle(item));
  }, [historyItems, articleById]);

  const todayFromArchivedUnread = useMemo(() => {
    const now = new Date();
    const merged: Article[] = [];

    for (const [idText, archivedAt] of Object.entries(archivedUnreadAt)) {
      if (!isSameCalendarDay(now, archivedAt)) {
        continue;
      }

      const article = articleById.get(Number(idText));
      if (article) {
        merged.push(article);
      }
    }

    return merged;
  }, [archivedUnreadAt, articleById]);

  const todayArticles = useMemo(() => {
    const merged = dedupeArticles([...todayFromHistory, ...todayFromArchivedUnread]);
    return merged.filter((article) => !unreadIdSet.has(article.id));
  }, [todayFromHistory, todayFromArchivedUnread, unreadIdSet]);

  const historyArticles = useMemo(() => {
    const now = new Date();
    return dedupeArticles(
      historyItems
        .filter((item) => !isSameCalendarDay(now, item.batchCreatedAt))
        .map((item) => articleById.get(item.id) ?? historyItemToArticle(item))
    );
  }, [historyItems, articleById]);

  const favoritePool = useMemo(
    () =>
      dedupeArticles([
        ...allArticles,
        ...historyItems.map((item) => articleById.get(item.id) ?? historyItemToArticle(item))
      ]),
    [allArticles, historyItems, articleById]
  );

  const favoriteArticles = useMemo(() => {
    return dedupeArticles(
      favoritePool.filter((article) => {
        if (usingDemoData) {
          return demoFavoriteIds.includes(article.id);
        }

        const hasNote = (article.note ?? "").trim().length > 0;
        return article.isFavorite || hasNote;
      })
    );
  }, [favoritePool, usingDemoData, demoFavoriteIds]);

  const buildArticleTree = (arts: Article[]) => {
    const tree = new Map<string, Map<string, number>>();

    for (const article of arts) {
      const meta = articleMetaById.get(article.id);
      if (!meta) {
        continue;
      }

      const module = meta.module || "other";
      const bucket = meta.bucket || "unspecified";
      if (!tree.has(module)) {
        tree.set(module, new Map());
      }

      const buckets = tree.get(module)!;
      buckets.set(bucket, (buckets.get(bucket) ?? 0) + 1);
    }

    return tree;
  };

  const todayTree = useMemo(() => buildArticleTree(todayArticles), [todayArticles, articleMetaById]);
  const favoritesTree = useMemo(
    () => buildArticleTree(favoriteArticles),
    [favoriteArticles, articleMetaById]
  );
  const historyTree = useMemo(() => buildArticleTree(historyArticles), [historyArticles, articleMetaById]);

  const todayTreeEntries = useMemo(() => orderedTreeEntries(todayTree), [todayTree]);
  const favoritesTreeEntries = useMemo(() => orderedTreeEntries(favoritesTree), [favoritesTree]);
  const historyTreeEntries = useMemo(() => orderedTreeEntries(historyTree), [historyTree]);

  const sectionCounts = useMemo(
    () => ({
      unread: unreadArticles.length,
      today: todayArticles.length,
      favorites: favoriteArticles.length,
      history: historyArticles.length
    }),
    [unreadArticles, todayArticles, favoriteArticles, historyArticles]
  );

  const timelineArticles = useMemo((): Article[] => {
    const matchByBucket = (article: Article, module: string, bucket: string) => {
      const meta = articleMetaById.get(article.id);
      return meta?.module === module && meta?.bucket === bucket;
    };

    if (selection.kind === "unread") {
      return [...unreadArticles].sort(
        (left, right) =>
          articleTimestamp(right, articleMetaById.get(right.id)?.pushedAt) -
          articleTimestamp(left, articleMetaById.get(left.id)?.pushedAt)
      );
    }

    const { section, module, bucket } = selection;
    if (section === "today") {
      return todayArticles
        .filter((article) => matchByBucket(article, module, bucket))
        .sort(
          (left, right) =>
            articleTimestamp(right, articleMetaById.get(right.id)?.pushedAt) -
            articleTimestamp(left, articleMetaById.get(left.id)?.pushedAt)
        );
    }

    if (section === "favorites") {
      return favoriteArticles
        .filter((article) => matchByBucket(article, module, bucket))
        .sort(
          (left, right) =>
            articleTimestamp(right, articleMetaById.get(right.id)?.pushedAt) -
            articleTimestamp(left, articleMetaById.get(left.id)?.pushedAt)
        );
    }

    if (section === "history") {
      return historyArticles
        .filter((article) => matchByBucket(article, module, bucket))
        .sort(
          (left, right) =>
            articleTimestamp(right, articleMetaById.get(right.id)?.pushedAt) -
            articleTimestamp(left, articleMetaById.get(left.id)?.pushedAt)
        );
    }

    return [];
  }, [selection, unreadArticles, todayArticles, favoriteArticles, historyArticles, articleMetaById]);

  const allKnownArticles = useMemo(
    () => dedupeArticles([...allArticles, ...todayArticles, ...favoriteArticles, ...historyArticles]),
    [allArticles, todayArticles, favoriteArticles, historyArticles]
  );

  const selectedArticle = useMemo(() => {
    const preferredId = localSelectedId ?? snapshot?.selectedArticleId ?? null;
    if (preferredId) {
      const inTimeline = timelineArticles.find((item) => item.id === preferredId);
      if (inTimeline) {
        return inTimeline;
      }

      const inAll = allKnownArticles.find((article) => article.id === preferredId);
      if (inAll) {
        return inAll;
      }
    }

    return timelineArticles[0] ?? null;
  }, [timelineArticles, allKnownArticles, localSelectedId, snapshot?.selectedArticleId]);

  const selectedArticleMeta = useMemo(() => {
    if (!selectedArticle) {
      return null;
    }
    return articleMetaById.get(selectedArticle.id) ?? null;
  }, [selectedArticle, articleMetaById]);

  useEffect(() => {
    const incoming = snapshot?.historyArticles ?? [];
    if (incoming.length === 0) {
      setHistoryArchive([]);
      setHistoryOffset(0);
      setHistoryHasMore(false);
      return;
    }

    const dedupIncoming = dedupeHistoryItems(incoming);
    setHistoryArchive((prev) => dedupeHistoryItems([...dedupIncoming, ...prev]));
    setHistoryOffset((prev) => Math.max(prev, dedupIncoming.length));
    setHistoryHasMore(dedupIncoming.length >= HISTORY_PAGE_SIZE);
  }, [snapshot?.historyArticles]);

  useEffect(() => {
    if (!snapshot || isConfigurationComplete || snapshot.activeView === "settings") {
      return;
    }

    void setActiveView("settings").then(setSnapshot);
  }, [snapshot, isConfigurationComplete, setSnapshot]);

  useEffect(() => {
    if (!selectedArticle) {
      return;
    }
    setLocalSelectedId(selectedArticle.id);
  }, [selectedArticle?.id]);

  useEffect(() => {
    const remoteSelectedId = snapshot?.selectedArticleId ?? null;
    if (!remoteSelectedId) {
      return;
    }

    setLocalSelectedId((current) =>
      current === remoteSelectedId ? current : remoteSelectedId
    );
  }, [snapshot?.selectedArticleId]);

  useEffect(() => {
    setNoteDraft(selectedArticle?.note ?? "");
  }, [selectedArticle?.id, selectedArticle?.note]);

  useEffect(() => {
    readerScrollRef.current?.scrollTo({ top: 0, behavior: "auto" });
  }, [selectedArticle?.id]);

  useEffect(() => {
    if (!selectedArticle || usingDemoData) {
      setRawContentLoading(false);
      return;
    }

    if (selectedRawArticleId === selectedArticle.id) {
      setRawContentLoading(false);
      return;
    }

    const articleId = selectedArticle.id;
    let cancelled = false;
    setRawContentLoading(true);
    setSelectedRawContent("");

    void getArticleRawContent(articleId)
      .then((content) => {
        if (cancelled) {
          return;
        }
        setSelectedRawContent(content);
        setSelectedRawArticleId(articleId);
      })
      .catch((err) => {
        if (cancelled) {
          return;
        }
        setInteractionMessage(err instanceof Error ? err.message : "加载原文失败");
      })
      .finally(() => {
        if (!cancelled) {
          setRawContentLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selectedArticle?.id, selectedRawArticleId, usingDemoData]);

  useEffect(() => {
    const updateCompact = () => {
      setIsCompact(window.innerWidth < 1120);
    };

    updateCompact();
    window.addEventListener("resize", updateCompact);
    return () => window.removeEventListener("resize", updateCompact);
  }, []);

  useEffect(() => {
    if (!isCompact) {
      return;
    }
    setSidebarCollapsed(true);
  }, [isCompact]);

  useEffect(() => {
    const onPointerMove = (event: PointerEvent) => {
      const target = resizeTargetRef.current;
      const host = layoutRef.current;
      if (!target || !host) {
        return;
      }

      const rect = host.getBoundingClientRect();
      const minReaderWidth = isCompact ? 300 : 420;
      const divider = 8;

      if (target === "sidebar") {
        const min = 210;
        const max = Math.max(
          min,
          rect.width - (timelineCollapsed ? 0 : middleWidth) - minReaderWidth - divider * 2
        );
        const next = Math.min(max, Math.max(min, event.clientX - rect.left));
        setLeftWidth(next);
      }

      if (target === "timeline") {
        const min = 280;
        const left = sidebarCollapsed ? 0 : leftWidth;
        const leftDivider = sidebarCollapsed ? 0 : divider;
        const max = Math.max(min, rect.width - left - leftDivider - minReaderWidth - divider);
        const next = Math.min(
          max,
          Math.max(min, event.clientX - rect.left - left - leftDivider)
        );
        setMiddleWidth(next);
      }
    };

    const onPointerUp = () => {
      resizeTargetRef.current = null;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);

    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
    };
  }, [leftWidth, middleWidth, sidebarCollapsed, timelineCollapsed, isCompact]);

  useEffect(() => {
    if (!interactionMessage) {
      return;
    }
    const timer = window.setTimeout(() => setInteractionMessage(null), 1800);
    return () => window.clearTimeout(timer);
  }, [interactionMessage]);

  function startResize(target: ResizeTarget, event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    resizeTargetRef.current = target;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }

  async function handleLoadMoreHistory() {
    if (usingDemoData || historyLoadingMore || !historyHasMore) {
      return;
    }

    setHistoryLoadingMore(true);
    try {
      const page = await listHistoryArticlesPage(historyOffset, HISTORY_PAGE_SIZE);
      setHistoryArchive((prev) => dedupeHistoryItems([...prev, ...page]));
      setHistoryOffset((prev) => prev + page.length);
      if (page.length < HISTORY_PAGE_SIZE) {
        setHistoryHasMore(false);
      }
    } catch (err) {
      setInteractionMessage(err instanceof Error ? err.message : "加载历史推送失败");
    } finally {
      setHistoryLoadingMore(false);
    }
  }

  async function handleSelectArticle(
    articleId: number,
    options?: { fromUnreadSelection?: boolean }
  ) {
    const unreadSelection = options?.fromUnreadSelection ?? selection.kind === "unread";
    setLocalSelectedId(articleId);

    if (unreadSelection) {
      const clicked = unreadClickCounts[articleId] ?? 0;
      if (clicked < 1) {
        setUnreadClickCounts({ [articleId]: clicked + 1 });
        setInteractionMessage("再次点击将归档到 Today");
        return;
      }

      setArchivedUnreadAt((prev) => ({
        ...prev,
        [articleId]: new Date().toISOString()
      }));
    }

    setUnreadClickCounts({});
    setManualUnreadIds((prev) => prev.filter((id) => id !== articleId));

    if (usingDemoData) {
      return;
    }

    try {
      const next = await openArticle(articleId);
      setSnapshot(next);
    } catch (err) {
      setInteractionMessage(err instanceof Error ? err.message : "打开文章失败");
    }
  }

  async function handleToggleFavorite() {
    if (!selectedArticle) {
      return;
    }

    if (usingDemoData) {
      setDemoFavoriteIds((prev) => {
        if (prev.includes(selectedArticle.id)) {
          return prev.filter((id) => id !== selectedArticle.id);
        }
        return [...prev, selectedArticle.id];
      });
      return;
    }

    try {
      const next = await toggleFavorite(selectedArticle.id);
      setSnapshot(next);
    } catch (err) {
      setInteractionMessage(err instanceof Error ? err.message : "收藏操作失败");
    }
  }

  async function handleSaveNote() {
    if (!selectedArticle) {
      return;
    }
    if (noteDraft.trim() === (selectedArticle.note ?? "").trim()) {
      return;
    }
    if (usingDemoData) {
      setInteractionMessage("Demo 模式不保存笔记");
      return;
    }

    setNoteSaving(true);
    try {
      const next = await saveArticleNote(selectedArticle.id, noteDraft);
      setSnapshot(next);
      setInteractionMessage("笔记已保存");
    } catch (err) {
      setInteractionMessage(err instanceof Error ? err.message : "保存笔记失败");
    } finally {
      setNoteSaving(false);
    }
  }

  function handleMarkUnread() {
    if (!selectedArticle) {
      return;
    }

    setSelection({ kind: "unread" });
    setArchivedUnreadAt((prev) => {
      if (!(selectedArticle.id in prev)) {
        return prev;
      }

      const next = { ...prev };
      delete next[selectedArticle.id];
      return next;
    });

    setManualUnreadIds((prev) => {
      if (prev.includes(selectedArticle.id)) {
        return prev;
      }
      return [...prev, selectedArticle.id];
    });
    setInteractionMessage("已标记为未读");
  }

  function handleOpenBrowser() {
    if (!selectedArticle) {
      return;
    }
    window.open(selectedArticle.link, "_blank", "noopener,noreferrer");
  }

  async function handleShare() {
    if (!selectedArticle) {
      return;
    }

    try {
      await navigator.clipboard.writeText(selectedArticle.link);
      setInteractionMessage("链接已复制");
    } catch {
      setInteractionMessage("复制失败，请手动复制链接");
    }
  }

  const selectedFeedLabel = useMemo(() => {
    if (selection.kind === "unread") return "Unread";
    const sectionLabel = { today: "Today", favorites: "Favorites", history: "History" }[selection.section];
    const modLabel = MODULE_LABELS[selection.module as SourceModule] ?? selection.module;
    const bktLabel = BUCKET_LABELS[selection.bucket as SourceBucket] ?? selection.bucket;
    return `${sectionLabel} · ${modLabel} / ${bktLabel}`;
  }, [selection]);

  const selectedModuleLabel = selectedArticleMeta
    ? MODULE_LABELS[selectedArticleMeta.module as SourceModule] ?? selectedArticleMeta.module
    : "未分类";
  const selectedBucketLabel = selectedArticleMeta
    ? BUCKET_LABELS[selectedArticleMeta.bucket as SourceBucket] ?? selectedArticleMeta.bucket
    : "未分类";
  const selectedSourceLabel = selectedArticleMeta?.sourceName ?? selectedArticle?.sourceName ?? "未知来源";
  const selectedPublishedTime = selectedArticle
    ? formatArticleTime(selectedArticle.publishedAt ?? selectedArticle.fetchedAt)
    : "未知";
  const selectedPushedTime = selectedArticleMeta?.pushedAt
    ? formatArticleTime(selectedArticleMeta.pushedAt)
    : "未记录";

  const layoutStyle = useMemo(() => {
    const sidebarWidth = sidebarCollapsed ? 0 : leftWidth;
    const timelineWidth = timelineCollapsed ? 0 : middleWidth;
    const sidebarDivider = sidebarCollapsed ? 0 : 8;
    const timelineDivider = timelineCollapsed ? 0 : 8;

    return {
      gridTemplateColumns: `${sidebarWidth}px ${sidebarDivider}px ${timelineWidth}px ${timelineDivider}px minmax(0, 1fr)`
    };
  }, [sidebarCollapsed, timelineCollapsed, leftWidth, middleWidth]);

  if (!snapshot) {
    if (loading) {
      return <div className="panel-empty main-window">正在初始化...</div>;
    }
    return <div className="panel-empty main-window">{error ?? "加载失败"}</div>;
  }

  return (
    <div className="main-window">
      <header className="main-header">
        <div className="main-header-left">
          <h1>Briefy Reader</h1>
          <span className={`status-pill status-${snapshot.petStatus}`}>
            {PET_STATUS_LABELS[snapshot.petStatus]}
          </span>
          <span className="main-meta">最近抓取 {formatTime(snapshot.lastScanAt)}</span>
        </div>

        <div className="main-header-actions">
          {snapshot.activeView === "reading" && (
            <>
              <button
                className={`toolbar-toggle ${sidebarCollapsed ? "" : "active"}`}
                onClick={() => setSidebarCollapsed((prev) => !prev)}
                aria-label="切换侧边栏"
              >
                <SymbolIcon name="sidebar" className="toolbar-icon" />
                边栏
              </button>
              <button
                className={`toolbar-toggle ${timelineCollapsed ? "" : "active"}`}
                onClick={() => setTimelineCollapsed((prev) => !prev)}
                aria-label="切换时间线"
              >
                <SymbolIcon name="timeline" className="toolbar-icon" />
                时间线
              </button>
            </>
          )}

          <button
            className={snapshot.activeView === "reading" ? "active" : ""}
            disabled={!isConfigurationComplete}
            title={!isConfigurationComplete ? "请先完成 API Key 与兴趣配置" : undefined}
            onClick={() => void setActiveView("reading").then(setSnapshot)}
          >
            阅读
          </button>
          <button
            className={snapshot.activeView === "settings" ? "active" : ""}
            onClick={() => void setActiveView("settings").then(setSnapshot)}
          >
            设置
          </button>
          <button onClick={() => void openHelpWindow()}>
            帮助
          </button>
        </div>
      </header>

      {snapshot.lastError && snapshot.lastError.toLowerCase().includes("api key") && (
        <div className="error-banner">{snapshot.lastError}</div>
      )}
      {interactionMessage && <div className="interaction-banner">{interactionMessage}</div>}
      {loading && <div className="loading-strip">正在同步最新快照...</div>}
      {snapshot.petStatus === "polling" && (
        <div className="loading-strip">正在轮询检查是否有到点信源或待提醒内容...</div>
      )}
      {snapshot.petStatus === "scanning" && (
        <div className="loading-strip">正在后台抓取并评分，本批次完成后会更新列表与提醒。</div>
      )}
      {error && <div className="error-banner">{error}</div>}

      {snapshot.activeView === "settings" ? (
        <SettingsView
          snapshot={snapshot}
          forceDisciplineSelection={!hasInterestedDiscipline}
          setSnapshot={setSnapshot}
        />
      ) : (
        <div className="reader-workbench">
          {!isConfigurationComplete ? (
            <div className="discipline-required-panel">
              <h2>先完成初始化配置</h2>
              <p>请先填写有效 API Key，并至少启用 1 个学科且写下兴趣偏好后再进入阅读。</p>
              <button onClick={() => void setActiveView("settings").then(setSnapshot)}>
                去设置
              </button>
            </div>
          ) : (
            <div ref={layoutRef} className="reader-layout" style={layoutStyle}>
            <aside className={`sidebar-pane ${sidebarCollapsed ? "collapsed" : ""}`}>
              <div className="pane-head">Feeds</div>

              <div className="sidebar-scroll">

                {/* ── Unread ─────────────────────────────── */}
                <section className="smart-section">
                  <button
                    className={`feed-row${selection.kind === "unread" ? " active" : ""}`}
                    onClick={() => setSelection({ kind: "unread" })}
                  >
                    <span className="feed-row-left">
                      <SymbolIcon name="unread" className="row-icon" />
                      <strong>Unread</strong>
                    </span>
                    {sectionCounts.unread > 0 && (
                      <span className="count-badge">{sectionCounts.unread}</span>
                    )}
                  </button>
                </section>

                {/* ── Today ──────────────────────────────── */}
                {(() => {
                  const sec: FeedSection = "today";
                  const entries = todayTreeEntries;
                  const secCollapsed = sectionCollapsed[sec];
                  return (
                    <section className="folder-section">
                      <button
                        className="folder-row section-head"
                        onClick={() =>
                          setSectionCollapsed((prev) => ({ ...prev, [sec]: !prev[sec] }))
                        }
                      >
                        <span className={`folder-chevron ${secCollapsed ? "collapsed" : ""}`}>
                          <SymbolIcon name="chevron" className="row-icon" />
                        </span>
                        <SymbolIcon name="today" className="row-icon" />
                        <span>Today</span>
                        {sectionCounts.today > 0 && (
                          <span className="count-badge">{sectionCounts.today}</span>
                        )}
                      </button>
                      {!secCollapsed &&
                        entries.map(({ module: mod, buckets }) => {
                          const fk = `${sec}:${mod}`;
                          const fc = collapsedFolders[fk] ?? false;
                          return (
                            <div key={fk} className="folder-block">
                              <button
                                className="folder-row module-row"
                                onClick={() =>
                                  setCollapsedFolders((prev) => ({ ...prev, [fk]: !fc }))
                                }
                              >
                                <span className={`folder-chevron ${fc ? "collapsed" : ""}`}>
                                  <SymbolIcon name="chevron" className="row-icon" />
                                </span>
                                <SymbolIcon name="folder" className="row-icon" />
                                <span>{MODULE_LABELS[mod as SourceModule] ?? mod}</span>
                              </button>
                              {!fc && (
                                <div className="feed-list">
                                  {buckets.map(([bkt, cnt]) => (
                                    <button
                                      key={bkt}
                                      className={`feed-row child${
                                        selection.kind === "bucket" &&
                                        selection.section === sec &&
                                        selection.module === mod &&
                                        selection.bucket === bkt
                                          ? " active"
                                          : ""
                                      }`}
                                      onClick={() =>
                                        setSelection({
                                          kind: "bucket",
                                          section: sec,
                                          module: mod,
                                          bucket: bkt
                                        })
                                      }
                                    >
                                      <span className="feed-row-left">
                                        <span className="feed-name">
                                          {BUCKET_LABELS[bkt as SourceBucket] ?? bkt}
                                        </span>
                                      </span>
                                      <span className="count-badge">{cnt}</span>
                                    </button>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        })}
                      {!secCollapsed && historyHasMore && (
                        <button
                          className="feed-row child"
                          disabled={historyLoadingMore}
                          onClick={() => void handleLoadMoreHistory()}
                        >
                          <span className="feed-row-left">
                            <span className="feed-name">
                              {historyLoadingMore ? "正在加载..." : "加载更多历史推送"}
                            </span>
                          </span>
                        </button>
                      )}
                    </section>
                  );
                })()}

                {/* ── Favorites ──────────────────────────── */}
                {(() => {
                  const sec: FeedSection = "favorites";
                  const entries = favoritesTreeEntries;
                  const secCollapsed = sectionCollapsed[sec];
                  return (
                    <section className="folder-section">
                      <button
                        className="folder-row section-head"
                        onClick={() =>
                          setSectionCollapsed((prev) => ({ ...prev, [sec]: !prev[sec] }))
                        }
                      >
                        <span className={`folder-chevron ${secCollapsed ? "collapsed" : ""}`}>
                          <SymbolIcon name="chevron" className="row-icon" />
                        </span>
                        <SymbolIcon name="star" className="row-icon" />
                        <span>Favorites</span>
                        {sectionCounts.favorites > 0 && (
                          <span className="count-badge">{sectionCounts.favorites}</span>
                        )}
                      </button>
                      {!secCollapsed &&
                        entries.map(({ module: mod, buckets }) => {
                          const fk = `${sec}:${mod}`;
                          const fc = collapsedFolders[fk] ?? false;
                          return (
                            <div key={fk} className="folder-block">
                              <button
                                className="folder-row module-row"
                                onClick={() =>
                                  setCollapsedFolders((prev) => ({ ...prev, [fk]: !fc }))
                                }
                              >
                                <span className={`folder-chevron ${fc ? "collapsed" : ""}`}>
                                  <SymbolIcon name="chevron" className="row-icon" />
                                </span>
                                <SymbolIcon name="folder" className="row-icon" />
                                <span>{MODULE_LABELS[mod as SourceModule] ?? mod}</span>
                              </button>
                              {!fc && (
                                <div className="feed-list">
                                  {buckets.map(([bkt, cnt]) => (
                                    <button
                                      key={bkt}
                                      className={`feed-row child${
                                        selection.kind === "bucket" &&
                                        selection.section === sec &&
                                        selection.module === mod &&
                                        selection.bucket === bkt
                                          ? " active"
                                          : ""
                                      }`}
                                      onClick={() =>
                                        setSelection({
                                          kind: "bucket",
                                          section: sec,
                                          module: mod,
                                          bucket: bkt
                                        })
                                      }
                                    >
                                      <span className="feed-row-left">
                                        <span className="feed-name">
                                          {BUCKET_LABELS[bkt as SourceBucket] ?? bkt}
                                        </span>
                                      </span>
                                      <span className="count-badge">{cnt}</span>
                                    </button>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        })}
                    </section>
                  );
                })()}

                {/* ── History ────────────────────────────── */}
                {(() => {
                  const sec: FeedSection = "history";
                  const entries = historyTreeEntries;
                  const secCollapsed = sectionCollapsed[sec];
                  return (
                    <section className="folder-section">
                      <button
                        className="folder-row section-head"
                        onClick={() =>
                          setSectionCollapsed((prev) => ({ ...prev, [sec]: !prev[sec] }))
                        }
                      >
                        <span className={`folder-chevron ${secCollapsed ? "collapsed" : ""}`}>
                          <SymbolIcon name="chevron" className="row-icon" />
                        </span>
                        <SymbolIcon name="reader" className="row-icon" />
                        <span>History</span>
                        <span className="count-badge">{sectionCounts.history}</span>
                      </button>
                      {!secCollapsed &&
                        entries.map(({ module: mod, buckets }) => {
                          const fk = `${sec}:${mod}`;
                          const fc = collapsedFolders[fk] ?? true;
                          return (
                            <div key={fk} className="folder-block">
                              <button
                                className="folder-row module-row"
                                onClick={() =>
                                  setCollapsedFolders((prev) => ({ ...prev, [fk]: !fc }))
                                }
                              >
                                <span className={`folder-chevron ${fc ? "collapsed" : ""}`}>
                                  <SymbolIcon name="chevron" className="row-icon" />
                                </span>
                                <SymbolIcon name="folder" className="row-icon" />
                                <span>{MODULE_LABELS[mod as SourceModule] ?? mod}</span>
                              </button>
                              {!fc && (
                                <div className="feed-list">
                                  {buckets.map(([bkt, cnt]) => (
                                    <button
                                      key={bkt}
                                      className={`feed-row child${
                                        selection.kind === "bucket" &&
                                        selection.section === sec &&
                                        selection.module === mod &&
                                        selection.bucket === bkt
                                          ? " active"
                                          : ""
                                      }`}
                                      onClick={() =>
                                        setSelection({
                                          kind: "bucket",
                                          section: sec,
                                          module: mod,
                                          bucket: bkt
                                        })
                                      }
                                    >
                                      <span className="feed-row-left">
                                        <span className="feed-name">
                                          {BUCKET_LABELS[bkt as SourceBucket] ?? bkt}
                                        </span>
                                      </span>
                                      <span className="count-badge">{cnt}</span>
                                    </button>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        })}
                    </section>
                  );
                })()}

              </div>

              <div className="sidebar-footer">
                <button
                  className="icon-button"
                  onClick={() => void setActiveView("settings").then(setSnapshot)}
                  aria-label="设置"
                >
                  <SymbolIcon name="settings" className="toolbar-icon" />
                </button>
                <button
                  className="icon-button"
                  onClick={() => setInteractionMessage("可在设置页管理和新增信源")}
                  aria-label="新增信源"
                >
                  <SymbolIcon name="add" className="toolbar-icon" />
                </button>
              </div>
            </aside>

            <div
              className={`column-divider ${sidebarCollapsed ? "hidden" : ""}`}
              onPointerDown={(event) => startResize("sidebar", event)}
            />

            <section className={`timeline-pane ${timelineCollapsed ? "collapsed" : ""}`}>
              <div className="pane-head timeline-head">
                <div>
                  <strong>{selectedFeedLabel}</strong>
                  <span>{timelineArticles.length} 篇</span>
                </div>
              </div>

              <div className="timeline-scroll">
                {timelineArticles.length === 0 && (
                  <div className="timeline-empty">当前筛选下暂无文章。</div>
                )}

                {timelineArticles.map((article) => {
                  const meta = articleMetaById.get(article.id);
                  const moduleLabel = meta
                    ? MODULE_LABELS[meta.module as SourceModule] ?? meta.module
                    : "未分类";
                  const bucketLabel = meta
                    ? BUCKET_LABELS[meta.bucket as SourceBucket] ?? meta.bucket
                    : "未分类";
                  const sourceLabel = meta?.sourceName ?? article.sourceName;
                  const timelineTime = formatArticleTime(
                    article.publishedAt ?? article.fetchedAt ?? meta?.pushedAt ?? null
                  );
                  const selected = selectedArticle?.id === article.id;
                  const isUnreadItem = unreadIdSet.has(article.id) || manualUnreadSet.has(article.id);

                  return (
                    <button
                      key={article.id}
                      className={`timeline-card ${selected ? "selected" : ""}`}
                      onClick={() => {
                        void handleSelectArticle(article.id, {
                          fromUnreadSelection: selection.kind === "unread"
                        });
                      }}
                    >
                      <div className="timeline-card-head">
                        <span className={`unread-dot ${isUnreadItem ? "visible" : ""}`} />
                        <h3>{article.title}</h3>
                      </div>
                      <p className="timeline-meta">
                        MOD {moduleLabel} · BKT {bucketLabel} · PUB {timelineTime}
                        {article.fitScore > 0 && (
                          <span className="score-chip"> · {article.fitScore}分</span>
                        )}
                      </p>
                      <p className="timeline-submeta">
                        SRC {sourceLabel} · FIT {fitLabel(article.fitLevel)}
                      </p>
                      <p className="timeline-snippet">{article.summary}</p>
                    </button>
                  );
                })}
              </div>
            </section>

            <div
              className={`column-divider ${timelineCollapsed ? "hidden" : ""}`}
              onPointerDown={(event) => startResize("timeline", event)}
            />

            <article className="reader-pane">
              {selectedArticle ? (
                <>
                  <header className="reader-header">
                    <div>
                      <h2>{selectedArticle.title}</h2>
                      <div className="reader-meta">
                        <span>MOD：{selectedModuleLabel}</span>
                        <span>BKT：{selectedBucketLabel}</span>
                        <span>PUB：{selectedPublishedTime}</span>
                        <span>PUSH：{selectedPushedTime}</span>
                        <button
                          className="feed-link"
                          onClick={() => {
                            if (selectedArticleMeta) {
                              const targetSection: FeedSection =
                                selection.kind === "bucket" ? selection.section : "today";
                              setSelection({
                                kind: "bucket",
                                section: targetSection,
                                module: selectedArticleMeta.module,
                                bucket: selectedArticleMeta.bucket
                              });
                            }
                          }}
                        >
                          SRC：{selectedSourceLabel}
                        </button>
                      </div>
                    </div>

                    <div className="reader-toolbar">
                      <button className="toolbar-button" onClick={handleMarkUnread}>
                        <SymbolIcon name="mark" className="toolbar-icon" />
                        标记未读
                      </button>
                      <button className="toolbar-button" onClick={() => void handleToggleFavorite()}>
                        <SymbolIcon name="star" className="toolbar-icon" />
                        {selectedArticle.isFavorite ? "取消星标" : "星标"}
                      </button>
                      <button className="toolbar-button" onClick={handleOpenBrowser}>
                        <SymbolIcon name="open" className="toolbar-icon" />
                        浏览器打开
                      </button>
                      <button className="toolbar-button" onClick={() => void handleShare()}>
                        <SymbolIcon name="share" className="toolbar-icon" />
                        分享
                      </button>
                    </div>
                  </header>

                  <div ref={readerScrollRef} className="reader-scroll">
                    <div className="reader-content-wrap">
                      <section className="reader-block">
                        <h3>CHECK 简表</h3>
                        <div className="reader-check-grid">
                          <p>
                            <strong>MOD</strong>
                            <span>{selectedModuleLabel}</span>
                          </p>
                          <p>
                            <strong>BKT</strong>
                            <span>{selectedBucketLabel}</span>
                          </p>
                          <p>
                            <strong>SRC</strong>
                            <span>{selectedSourceLabel}</span>
                          </p>
                          <p>
                            <strong>PUB</strong>
                            <span>{selectedPublishedTime}</span>
                          </p>
                          <p>
                            <strong>PUSH</strong>
                            <span>{selectedPushedTime}</span>
                          </p>
                          <p>
                            <strong>FIT</strong>
                            <span>
                              {fitLabel(selectedArticle.fitLevel)} / {selectedArticle.fitScore} 分
                            </span>
                          </p>
                          <p>
                            <strong>FAV</strong>
                            <span>{selectedArticle.isFavorite ? "是" : "否"}</span>
                          </p>
                          <p>
                            <strong>NOTE</strong>
                            <span>{(selectedArticle.note ?? "").trim() ? "有" : "无"}</span>
                          </p>
                        </div>
                      </section>

                      <section className="reader-block">
                        <h3>LLM摘要</h3>
                        <p>{selectedArticle.summary || "暂无摘要"}</p>
                      </section>

                      <section className="reader-block">
                        <h3>LLM判断与用户兴趣打分原因</h3>
                        <p className="reader-score-line">
                          契合度 {fitLabel(selectedArticle.fitLevel)} · {selectedArticle.fitScore} 分
                        </p>
                        <p>{selectedArticle.recommendationReason || "暂无推荐理由"}</p>
                      </section>

                      <section className="reader-block">
                        <h3>我的笔记</h3>
                        <textarea
                          className="reader-note-editor"
                          value={noteDraft}
                          onChange={(event) => setNoteDraft(event.target.value)}
                          onBlur={() => void handleSaveNote()}
                          placeholder="记录你的想法，失焦后自动保存"
                        />
                        <p className="reader-score-line">
                          {noteSaving ? "正在保存笔记..." : "提示：在 Unread 视图中双击同一条内容可归档"}
                        </p>
                      </section>

                      <section className="reader-block">
                        <h3>RSS抓到的原文</h3>
                        <pre className="reader-raw-content">
                          {rawContentLoading
                            ? "正在加载原文..."
                            : rssRawToText(
                                usingDemoData
                                  ? selectedArticle.rawContent || ""
                                  : selectedRawContent || ""
                              ) || "暂无原文内容"}
                        </pre>
                        <a href={selectedArticle.link} target="_blank" rel="noreferrer">
                          查看原文链接
                        </a>
                      </section>
                    </div>
                  </div>
                </>
              ) : (
                <div className="reader-empty">
                  <SymbolIcon name="reader" className="reader-empty-icon" />
                  <p>选择一篇文章开始阅读</p>
                </div>
              )}
            </article>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function App() {
  const [windowLabel, setWindowLabel] = useState(() => appWindow.label);
  const usesSnapshotWindow = windowLabel === "main" || windowLabel === "memory-review";
  const { snapshot, setSnapshot, loading, error } = useSnapshotEvents(usesSnapshotWindow);
  const overlaySnapshot = useOverlayEvents(!usesSnapshotWindow);

  useEffect(() => {
    const label = appWindow.label;
    setWindowLabel(label);
    document.documentElement.dataset.window = label;
    document.body.dataset.window = label;
    return () => {
      delete document.documentElement.dataset.window;
      delete document.body.dataset.window;
    };
  }, []);

  useEffect(() => {
    if (windowLabel !== "pet") {
      return;
    }

    const preventWheel = (event: WheelEvent) => {
      event.preventDefault();
    };

    window.addEventListener("wheel", preventWheel, { passive: false });
    return () => {
      window.removeEventListener("wheel", preventWheel);
    };
  }, [windowLabel]);

  if (windowLabel === "pet") {
    return <PetWindow snapshot={overlaySnapshot} />;
  }

  if (windowLabel === "bubble") {
    return <BubbleWindow snapshot={overlaySnapshot} />;
  }

  if (windowLabel === "help") {
    return <HelpWindow />;
  }

  if (windowLabel === "memory-review") {
    return <MemoryReviewWindow proposal={snapshot?.memoryReview} />;
  }

  return (
    <MainWindow
      snapshot={snapshot}
      setSnapshot={setSnapshot}
      loading={loading}
      error={error}
    />
  );
}
