import { useEffect, useMemo, useRef, useState } from "react";
import { appWindow } from "@tauri-apps/api/window";
import {
  bootstrap,
  bubbleAction,
  openArticle,
  petDoubleClick,
  saveSettings,
  setActiveView,
  toggleFavorite
} from "./api";
import type {
  AppView,
  Article,
  Discipline,
  FitLevel,
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
  scanning: "扫描中",
  idle: "待命中",
  "new-info": "新提醒"
} as const;

const PET_STATUS_HINTS = {
  loading: "",
  "needs-config": "先去完善配置",
  scanning: "正在按学科与子分类调度抓取",
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
    isFavorite: false,
    isNew: true
  }
];

type SmartFeedKey = "today" | "unread" | "starred";

type FeedSelection =
  | { kind: "smart"; key: SmartFeedKey }
  | { kind: "source"; sourceId: string };

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

function useSnapshotPolling(enabled: boolean, intervalMs: number) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      const next = await bootstrap();
      setSnapshot(next);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (!enabled) {
      return;
    }

    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, intervalMs);

    return () => window.clearInterval(timer);
  }, [enabled, intervalMs]);

  return { snapshot, setSnapshot, loading, error };
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

function articleTimestamp(article: Article) {
  const raw = article.publishedAt ?? article.fetchedAt;
  if (!raw) {
    return 0;
  }
  return new Date(raw).getTime();
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

function Favicon({ url, name }: { url: string; name: string }) {
  const [broken, setBroken] = useState(false);

  let host = "";
  try {
    host = new URL(url).hostname;
  } catch {
    host = "";
  }

  const fallbackText = (name.trim().charAt(0) || "•").toUpperCase();

  if (broken || !host) {
    return <span className="feed-favicon feed-favicon-fallback">{fallbackText}</span>;
  }

  return (
    <img
      className="feed-favicon"
      src={`https://www.google.com/s2/favicons?domain=${host}&sz=16`}
      alt=""
      loading="lazy"
      onError={() => setBroken(true)}
    />
  );
}

function PetWindow({ snapshot }: { snapshot: Snapshot | null }) {
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

function BubbleWindow({ snapshot }: { snapshot: Snapshot | null }) {
  const reminder = snapshot?.activeReminder;

  if (!reminder) {
    return <div className="bubble-empty" />;
  }

  return (
    <div className="bubble-window">
      <div className="bubble-card">
        <p className="bubble-kicker">Briefy-pet 提醒</p>
        <h2>你有 {reminder.articleCount} 条新内容</h2>
        <p className="bubble-copy">
          当前提醒跨 {reminder.partitionCount} 个分区，桌宠已经切到提醒状态。你可以立刻进入主界面，也可以只延后当前这一批。
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

  const disciplinePrefs = useMemo(
    () => sortDisciplinePrefs(snapshot.settings.disciplines),
    [snapshot.settings.disciplines]
  );

  const groupedSources = useMemo(
    () => groupSources(snapshot.settings.rssSources),
    [snapshot.settings.rssSources]
  );

  async function handleSave(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const formData = new FormData(event.currentTarget);
    const disciplines = disciplinePrefs.map((item) => ({
      discipline: item.discipline,
      enabled: formData.get(`discipline-enabled-${item.discipline}`) === "on",
      preference: String(formData.get(`discipline-pref-${item.discipline}`) ?? "")
    }));

    if (!disciplines.some((item) => item.enabled)) {
      setSubmitError("请至少选择 1 个感兴趣学科，才能进入三栏阅读模式。");
      return;
    }

    const rssSources = snapshot.settings.rssSources.map((source) => ({
      ...source,
      enabled: formData.get(`source-${source.id}`) === "on"
    }));

    const payload: SettingsPayload = {
      apiKey: String(formData.get("apiKey") ?? ""),
      autoStart: formData.get("autoStart") === "on",
      disciplines,
      memoryModeEnabled: formData.get("memoryModeEnabled") === "on",
      memorySummary: String(formData.get("memorySummary") ?? ""),
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
          <p>设置有效 API Key 后系统才会进入抓取和评分流程。</p>
        </div>
        <label>
          <span>API Key</span>
          <input
            name="apiKey"
            type="password"
            defaultValue={snapshot.settings.apiKey}
            placeholder="输入你的 API Key"
          />
        </label>
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
                placeholder={`写下你在 ${DISCIPLINE_LABELS[item.discipline]} 方向最想收到的内容`}
              />
            </div>
          ))}
        </div>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>每日兴趣记忆</h2>
          <p>你可以启用自动记忆，也可以手动编辑记忆摘要。</p>
        </div>
        <label className="checkbox-row">
          <input
            name="memoryModeEnabled"
            type="checkbox"
            defaultChecked={snapshot.settings.memoryModeEnabled}
          />
          <span>启用每日兴趣记忆</span>
        </label>
        <label>
          <span>记忆摘要</span>
          <textarea
            name="memorySummary"
            defaultValue={snapshot.settings.memorySummary}
            placeholder="系统会在这里生成当天记忆"
          />
        </label>
      </section>

      <section className="settings-card">
        <div className="settings-section-head">
          <h2>源池开关</h2>
          <p>按 module 与 bucket 分组管理信源开关。</p>
        </div>
        <div className="source-groups">
          {MODULE_ORDER.filter((module) => groupedSources.has(module)).map((module) => (
            <section key={module} className="source-discipline-block">
              <div className="source-discipline-head">
                <h3>{MODULE_LABELS[module]}</h3>
              </div>
              {Array.from(groupedSources.get(module)!.entries()).map(([bucket, sources]) => (
                <div key={`${module}-${bucket}`} className="source-kind-block">
                  <div className="source-kind-head">
                    <strong>{BUCKET_LABELS[bucket]}</strong>
                    <span>
                      {SOURCE_KIND_LABELS[sources[0].sourceKind]} · {sources.length} 个
                    </span>
                  </div>
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
                </div>
              ))}
            </section>
          ))}
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
  const [selection, setSelection] = useState<FeedSelection>({ kind: "smart", key: "unread" });
  const [collapsedFolders, setCollapsedFolders] = useState<Record<string, boolean>>({});
  const [manualUnreadIds, setManualUnreadIds] = useState<number[]>([]);
  const [demoFavoriteIds, setDemoFavoriteIds] = useState<number[]>(
    DEMO_ARTICLES.filter((item) => item.isFavorite).map((item) => item.id)
  );
  const [localSelectedId, setLocalSelectedId] = useState<number | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [timelineCollapsed, setTimelineCollapsed] = useState(false);
  const [leftWidth, setLeftWidth] = useState(252);
  const [middleWidth, setMiddleWidth] = useState(332);
  const [interactionMessage, setInteractionMessage] = useState<string | null>(null);
  const [isCompact, setIsCompact] = useState(false);

  const layoutRef = useRef<HTMLDivElement | null>(null);
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

  const baseArticles = useMemo(() => {
    const all = usingDemoData ? DEMO_ARTICLES : snapshot?.articles ?? [];

    if (!hasInterestedDiscipline) {
      return [];
    }

    return all.filter((article) => {
      if (activeSourceIds.has(article.sourceId)) {
        return true;
      }
      return interestedDisciplines.has(article.discipline);
    });
  }, [snapshot?.articles, usingDemoData, hasInterestedDiscipline, activeSourceIds, interestedDisciplines]);

  const articles = useMemo(() => {
    if (!usingDemoData) {
      return baseArticles;
    }
    const favoriteSet = new Set(demoFavoriteIds);
    return baseArticles.map((item) => ({
      ...item,
      isFavorite: favoriteSet.has(item.id)
    }));
  }, [baseArticles, demoFavoriteIds, usingDemoData]);

  const manualUnreadSet = useMemo(() => new Set(manualUnreadIds), [manualUnreadIds]);

  const isArticleUnread = (article: Article) => article.isNew || manualUnreadSet.has(article.id);

  const folders = useMemo(() => {
    const grouped = new Map<SourceModule, RssSource[]>();
    for (const source of activeSources) {
      if (!grouped.has(source.module)) {
        grouped.set(source.module, []);
      }
      grouped.get(source.module)!.push(source);
    }

    const sorted = new Map<SourceModule, RssSource[]>();
    for (const module of MODULE_ORDER) {
      const items = grouped.get(module);
      if (!items || items.length === 0) {
        continue;
      }
      sorted.set(
        module,
        [...items].sort((a, b) => {
          const bucketDiff = BUCKET_ORDER.indexOf(a.bucket) - BUCKET_ORDER.indexOf(b.bucket);
          if (bucketDiff !== 0) {
            return bucketDiff;
          }
          return a.name.localeCompare(b.name);
        })
      );
    }

    return sorted;
  }, [activeSources]);

  const unreadBySource = useMemo(() => {
    const map = new Map<string, number>();
    for (const article of articles) {
      if (!isArticleUnread(article)) {
        continue;
      }
      map.set(article.sourceId, (map.get(article.sourceId) ?? 0) + 1);
    }
    return map;
  }, [articles, manualUnreadSet]);

  const smartCounts = useMemo(() => {
    const now = new Date();
    let today = 0;
    let unread = 0;
    let starred = 0;

    for (const article of articles) {
      const date = new Date(article.publishedAt ?? article.fetchedAt ?? 0);
      if (
        date.getFullYear() === now.getFullYear() &&
        date.getMonth() === now.getMonth() &&
        date.getDate() === now.getDate()
      ) {
        today += 1;
      }
      if (isArticleUnread(article)) {
        unread += 1;
      }
      if (article.isFavorite) {
        starred += 1;
      }
    }

    return { today, unread, starred };
  }, [articles, manualUnreadSet]);

  const timelineArticles = useMemo(() => {
    let result = [...articles];

    if (selection.kind === "smart") {
      if (selection.key === "today") {
        const now = new Date();
        result = result.filter((article) => {
          const date = new Date(article.publishedAt ?? article.fetchedAt ?? 0);
          return (
            date.getFullYear() === now.getFullYear() &&
            date.getMonth() === now.getMonth() &&
            date.getDate() === now.getDate()
          );
        });
      }
      if (selection.key === "unread") {
        result = result.filter((article) => isArticleUnread(article));
      }
      if (selection.key === "starred") {
        result = result.filter((article) => article.isFavorite);
      }
    }

    if (selection.kind === "source") {
      result = result.filter((article) => article.sourceId === selection.sourceId);
    }

    result.sort((left, right) => {
      const unreadDiff = Number(isArticleUnread(right)) - Number(isArticleUnread(left));
      if (unreadDiff !== 0) {
        return unreadDiff;
      }
      return articleTimestamp(right) - articleTimestamp(left);
    });

    return result;
  }, [articles, selection, manualUnreadSet]);

  const selectedArticle = useMemo(() => {
    const preferredId = usingDemoData ? localSelectedId : snapshot?.selectedArticleId ?? localSelectedId;
    if (preferredId) {
      const found = timelineArticles.find((item) => item.id === preferredId);
      if (found) {
        return found;
      }
    }
    return timelineArticles[0] ?? null;
  }, [timelineArticles, usingDemoData, localSelectedId, snapshot?.selectedArticleId]);

  useEffect(() => {
    if (!selection || selection.kind !== "source") {
      return;
    }
    const exists = activeSources.some((source) => source.id === selection.sourceId);
    if (!exists) {
      setSelection({ kind: "smart", key: "unread" });
    }
  }, [activeSources, selection]);

  useEffect(() => {
    if (!snapshot || hasInterestedDiscipline || snapshot.activeView === "settings") {
      return;
    }

    void setActiveView("settings").then(setSnapshot);
  }, [snapshot, hasInterestedDiscipline, setSnapshot]);

  useEffect(() => {
    if (!selectedArticle) {
      return;
    }
    setLocalSelectedId(selectedArticle.id);
  }, [selectedArticle?.id]);

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

  async function handleSelectArticle(articleId: number) {
    setManualUnreadIds((prev) => prev.filter((id) => id !== articleId));

    if (usingDemoData) {
      setLocalSelectedId(articleId);
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

  function handleMarkUnread() {
    if (!selectedArticle) {
      return;
    }
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
    if (selection.kind === "smart") {
      if (selection.key === "today") {
        return "Today";
      }
      if (selection.key === "unread") {
        return "Unread";
      }
      return "Starred";
    }

    return activeSources.find((source) => source.id === selection.sourceId)?.name ?? "Feed";
  }, [activeSources, selection]);

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
            disabled={!hasInterestedDiscipline}
            title={!hasInterestedDiscipline ? "请先在设置中选择至少一个感兴趣学科" : undefined}
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
        </div>
      </header>

      {snapshot.lastError && <div className="error-banner">{snapshot.lastError}</div>}
      {interactionMessage && <div className="interaction-banner">{interactionMessage}</div>}
      {loading && <div className="loading-strip">正在同步最新快照...</div>}
      {error && <div className="error-banner">{error}</div>}

      {snapshot.activeView === "settings" ? (
        <SettingsView
          snapshot={snapshot}
          forceDisciplineSelection={!hasInterestedDiscipline}
          setSnapshot={setSnapshot}
        />
      ) : (
        <div className="reader-workbench">
          {!hasInterestedDiscipline ? (
            <div className="discipline-required-panel">
              <h2>先选择你感兴趣的学科</h2>
              <p>完成后，左侧订阅池将只保留对应学科的信源，不再展示全部来源。</p>
              <button onClick={() => void setActiveView("settings").then(setSnapshot)}>
                去设置学科
              </button>
            </div>
          ) : (
            <div ref={layoutRef} className="reader-layout" style={layoutStyle}>
            <aside className={`sidebar-pane ${sidebarCollapsed ? "collapsed" : ""}`}>
              <div className="pane-head">Feeds</div>

              <div className="sidebar-scroll">
                <section className="smart-section">
                  <h2>Smart Feeds</h2>

                  <button
                    className={`feed-row ${selection.kind === "smart" && selection.key === "today" ? "active" : ""}`}
                    onClick={() => setSelection({ kind: "smart", key: "today" })}
                  >
                    <span className="feed-row-left">
                      <SymbolIcon name="today" className="row-icon" />
                      Today
                    </span>
                    <span className="count-badge">{smartCounts.today}</span>
                  </button>

                  <button
                    className={`feed-row ${selection.kind === "smart" && selection.key === "unread" ? "active" : ""}`}
                    onClick={() => setSelection({ kind: "smart", key: "unread" })}
                  >
                    <span className="feed-row-left">
                      <SymbolIcon name="unread" className="row-icon" />
                      Unread
                    </span>
                    <span className="count-badge">{smartCounts.unread}</span>
                  </button>

                  <button
                    className={`feed-row ${selection.kind === "smart" && selection.key === "starred" ? "active" : ""}`}
                    onClick={() => setSelection({ kind: "smart", key: "starred" })}
                  >
                    <span className="feed-row-left">
                      <SymbolIcon name="star" className="row-icon" />
                      Starred
                    </span>
                    <span className="count-badge">{smartCounts.starred}</span>
                  </button>
                </section>

                <section className="folder-section">
                  <h2>Folders</h2>

                  {Array.from(folders.entries()).map(([module, sources]) => {
                    const folderKey = module;
                    const collapsed = collapsedFolders[folderKey] ?? false;

                    return (
                      <div key={folderKey} className="folder-block">
                        <button
                          className="folder-row"
                          onClick={() => {
                            setCollapsedFolders((prev) => ({
                              ...prev,
                              [folderKey]: !collapsed
                            }));
                          }}
                        >
                          <span className={`folder-chevron ${collapsed ? "collapsed" : ""}`}>
                            <SymbolIcon name="chevron" className="row-icon" />
                          </span>
                          <SymbolIcon name="folder" className="row-icon" />
                          <span>{MODULE_LABELS[module]}</span>
                        </button>

                        {!collapsed && (
                          <div className="feed-list">
                            {sources.map((source) => {
                              const selected =
                                selection.kind === "source" && selection.sourceId === source.id;
                              const unreadCount = unreadBySource.get(source.id) ?? 0;

                              return (
                                <button
                                  key={source.id}
                                  className={`feed-row child ${selected ? "active" : ""}`}
                                  onClick={() => setSelection({ kind: "source", sourceId: source.id })}
                                >
                                  <span className="feed-row-left">
                                    <Favicon url={source.url} name={source.name} />
                                    <span className="feed-name">{source.name}</span>
                                  </span>
                                  {unreadCount > 0 && (
                                    <span className="count-badge">{unreadCount}</span>
                                  )}
                                </button>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </section>
              </div>

              <div className="sidebar-footer">
                <button
                  className="icon-button"
                  onClick={() => {
                    void setActiveView("settings").then(setSnapshot);
                  }}
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
                  const selected = selectedArticle?.id === article.id;
                  const unread = isArticleUnread(article);

                  return (
                    <button
                      key={article.id}
                      className={`timeline-card ${selected ? "selected" : ""}`}
                      onClick={() => {
                        void handleSelectArticle(article.id);
                      }}
                    >
                      <div className="timeline-card-head">
                        <span className={`unread-dot ${unread ? "visible" : ""}`} />
                        <h3>{article.title}</h3>
                      </div>
                      <p className="timeline-meta">
                        {article.sourceName} · {formatTimelineTime(article.publishedAt ?? article.fetchedAt)}
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
                        <span>作者：{selectedArticle.sourceName}</span>
                        <span>时间：{formatArticleTime(selectedArticle.publishedAt)}</span>
                        <button
                          className="feed-link"
                          onClick={() =>
                            setSelection({ kind: "source", sourceId: selectedArticle.sourceId })
                          }
                        >
                          来源：{selectedArticle.sourceName}
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

                  <div className="reader-scroll">
                    <div className="reader-content-wrap">
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
                        <h3>RSS抓到的原文</h3>
                        <pre className="reader-raw-content">
                          {rssRawToText(selectedArticle.rawContent || "") || "暂无原文内容"}
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
  const pollIntervalMs = windowLabel === "main" ? 1800 : 400;
  const { snapshot, setSnapshot, loading, error } = useSnapshotPolling(true, pollIntervalMs);

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
    return <PetWindow snapshot={snapshot} />;
  }

  if (windowLabel === "bubble") {
    return <BubbleWindow snapshot={snapshot} />;
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
