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
  Article,
  ContentPoolStat,
  Discipline,
  FitLevel,
  RssSource,
  SettingsPayload,
  Snapshot,
  SourceKind,
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
  scanning: "正在按源类型调度抓取",
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

function fitLabel(level: FitLevel) {
  if (level === "high") {
    return "高";
  }
  if (level === "medium") {
    return "中";
  }
  return "低";
}

function sortDisciplinePrefs(items: UserDisciplinePreference[]) {
  return [...items].sort(
    (left, right) =>
      DISCIPLINE_ORDER.indexOf(left.discipline) - DISCIPLINE_ORDER.indexOf(right.discipline)
  );
}

function groupSources(sources: RssSource[]) {
  const grouped = new Map<Discipline, Map<SourceKind, RssSource[]>>();
  for (const source of sources) {
    if (!grouped.has(source.discipline)) {
      grouped.set(source.discipline, new Map());
    }
    const disciplineGroup = grouped.get(source.discipline)!;
    if (!disciplineGroup.has(source.sourceKind)) {
      disciplineGroup.set(source.sourceKind, []);
    }
    disciplineGroup.get(source.sourceKind)!.push(source);
  }
  return grouped;
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

function ArticleGroup({
  title,
  articles,
  selectedId,
  onOpen
}: {
  title: string;
  articles: Article[];
  selectedId: number | null;
  onOpen: (articleId: number) => void;
}) {
  return (
    <section className="article-group">
      <div className="group-head">
        <h2>{title}</h2>
        <span>{articles.length}</span>
      </div>
      {articles.length === 0 && <div className="group-empty">暂无内容</div>}
      {articles.map((article) => (
        <button
          key={`${title}-${article.id}`}
          className={`article-card ${selectedId === article.id ? "selected" : ""}`}
          onClick={() => onOpen(article.id)}
        >
          <span className="article-card-title">{article.title}</span>
          <span className="article-card-subtitle">
            {article.sourceName} · {DISCIPLINE_LABELS[article.discipline]}
          </span>
          <span className="article-card-meta">
            <span>{article.fitScore} 分</span>
            <span>{SOURCE_KIND_LABELS[article.sourceKind]}</span>
            {article.isFavorite && <span>已收藏</span>}
          </span>
        </button>
      ))}
    </section>
  );
}

function SourceStatCard({ stat }: { stat: ContentPoolStat }) {
  return (
    <div className="pool-stat-card">
      <strong>{SOURCE_KIND_LABELS[stat.sourceKind]}</strong>
      <span>{stat.totalArticles} 条分区池内容</span>
      <span>提醒候选 {stat.candidateCount} 条</span>
      <span>最高分 {stat.topScore ?? "暂无"}</span>
    </div>
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
  const [saving, setSaving] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const groupedArticles = useMemo(
    () => ({
      fresh: (snapshot?.articles ?? []).filter((article) => article.isNew),
      all: snapshot?.articles ?? [],
      favorite: (snapshot?.articles ?? []).filter((article) => article.isFavorite)
    }),
    [snapshot?.articles]
  );

  const selectedArticle =
    snapshot?.articles.find((article) => article.id === snapshot.selectedArticleId) ??
    groupedArticles.fresh[0] ??
    groupedArticles.all[0] ??
    null;

  const disciplinePrefs = useMemo(
    () => sortDisciplinePrefs(snapshot?.settings.disciplines ?? []),
    [snapshot?.settings.disciplines]
  );

  const groupedSources = useMemo(
    () => groupSources(snapshot?.settings.rssSources ?? []),
    [snapshot?.settings.rssSources]
  );

  async function handleSave(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!snapshot) {
      return;
    }

    const formData = new FormData(event.currentTarget);
    const disciplines = disciplinePrefs.map((item) => ({
      discipline: item.discipline,
      enabled: formData.get(`discipline-enabled-${item.discipline}`) === "on",
      preference: String(formData.get(`discipline-pref-${item.discipline}`) ?? "")
    }));
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
    <div className="main-window app-surface">
      <header className="topbar">
        <div className="topbar-copy">
          <h1>Briefy-pet</h1>
          <p>面向普通用户的信息雷达桌宠，按学科偏好和源类型进行分层提醒。</p>
        </div>
        <div className="topbar-actions">
          <span className={`status-badge status-${snapshot?.petStatus ?? "loading"}`}>
            {PET_STATUS_LABELS[snapshot?.petStatus ?? "loading"]}
          </span>
          <button
            className={snapshot?.activeView === "reading" ? "active" : ""}
            onClick={() => void setActiveView("reading").then(setSnapshot)}
          >
            阅读
          </button>
          <button
            className={snapshot?.activeView === "settings" ? "active" : ""}
            onClick={() => void setActiveView("settings").then(setSnapshot)}
          >
            设置
          </button>
        </div>
      </header>

      <section className="overview-strip">
        <div className="overview-card">
          <span className="overview-label">提醒状态</span>
          <strong>
            {snapshot?.activeReminder
              ? `${snapshot.activeReminder.articleCount} 条待提醒 / ${snapshot.activeReminder.partitionCount} 个分区`
              : "当前无提醒"}
          </strong>
        </div>
        <div className="overview-card">
          <span className="overview-label">目录与订阅池</span>
          <strong>
            {snapshot?.sourceSummary.enabledSources ?? 0} / {snapshot?.sourceSummary.totalSources ?? 0} 个有效信源
          </strong>
        </div>
        <div className="overview-card">
          <span className="overview-label">最近抓取</span>
          <strong>{formatTime(snapshot?.lastScanAt ?? null)}</strong>
        </div>
      </section>

      <section className="overview-strip compact">
        <div className="overview-card">
          <span className="overview-label">待调度信源</span>
          <strong>{snapshot?.sourceSummary.dueSources ?? 0}</strong>
        </div>
        <div className="overview-card">
          <span className="overview-label">已选学科</span>
          <strong>{snapshot?.sourceSummary.selectedDisciplines ?? 0}</strong>
        </div>
        <div className="overview-card">
          <span className="overview-label">目录异常项</span>
          <strong>{snapshot?.sourceSummary.postponedSources ?? 0}</strong>
        </div>
      </section>

      {snapshot?.lastError && <div className="error-banner">{snapshot.lastError}</div>}
      {submitError && <div className="error-banner">{submitError}</div>}
      {loading && <div className="panel-empty">正在加载...</div>}
      {error && <div className="panel-empty">{error}</div>}

      {!loading && !error && snapshot?.activeView === "settings" && (
        <form className="settings-view" onSubmit={handleSave}>
          <section className="settings-card">
            <div className="settings-section-head">
              <h2>启动门槛</h2>
              <p>本版必须同时满足 API Key、至少 1 个有效学科、以及每个已选学科的偏好描述，系统才会进入抓取与推送。</p>
            </div>
            <label>
              <span>API Key 设置</span>
              <input
                name="apiKey"
                type="password"
                defaultValue={snapshot.settings.apiKey}
                placeholder="输入你的 API Key"
              />
            </label>
            <label className="checkbox-row">
              <input
                name="autoStart"
                type="checkbox"
                defaultChecked={snapshot.settings.autoStart}
              />
              <span>开机启动</span>
            </label>
          </section>

          <section className="settings-card">
            <div className="settings-section-head">
              <h2>结构化兴趣</h2>
              <p>勾选需要运营的模块，并为每个已选模块写 1 到 2 句偏好。系统会按模块和源桶完成抓取与排序。</p>
            </div>
            <div className="discipline-grid">
              {disciplinePrefs.map((item) => {
                return (
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
                      placeholder={`写下你在${DISCIPLINE_LABELS[item.discipline]}方向真正想收到的内容`}
                    />
                  </div>
                );
              })}
            </div>
          </section>

          <section className="settings-card">
            <div className="settings-section-head">
              <h2>每日兴趣记忆</h2>
              <p>系统会根据打开详情、收藏和提醒操作汇总出每日兴趣总结。你可以关闭自动记忆，也可以直接编辑当前摘要。</p>
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
              <span>当前兴趣摘要</span>
              <textarea
                name="memorySummary"
                defaultValue={snapshot.settings.memorySummary}
                placeholder="系统会在这里生成或保留你确认后的兴趣摘要"
              />
            </label>
            {snapshot.memory && (
              <div className="memory-card">
                <strong>{snapshot.memory.dayKey}</strong>
                <p>{snapshot.memory.generatedSummary}</p>
                <span>今日行为计数：{snapshot.memory.eventCount}</span>
              </div>
            )}
          </section>

          <section className="settings-card">
            <div className="settings-section-head">
              <h2>源池开关</h2>
              <p>源按模块和二级源桶分组，所有模块均参与当前版本的抓取与提醒。</p>
            </div>
            <div className="source-groups">
              {DISCIPLINE_ORDER.filter((discipline) => groupedSources.has(discipline)).map((discipline) => (
                <section key={discipline} className="source-discipline-block">
                  <div className="source-discipline-head">
                    <h3>{DISCIPLINE_LABELS[discipline]}</h3>
                  </div>
                  {Array.from(groupedSources.get(discipline)!.entries()).map(([sourceKind, sources]) => (
                    <div key={`${discipline}-${sourceKind}`} className="source-kind-block">
                      <div className="source-kind-head">
                        <strong>{SOURCE_KIND_LABELS[sourceKind]}</strong>
                        <span>{sources.length} 个</span>
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
              {saving ? "验证并保存中..." : "保存并重新进入调度"}
            </button>
          </div>
        </form>
      )}

      {!loading && !error && snapshot?.activeView === "reading" && (
        <div className="reading-view">
          <aside className="article-list-panel">
            <ArticleGroup
              title="新内容"
              articles={groupedArticles.fresh}
              selectedId={snapshot.selectedArticleId}
              onOpen={(articleId) => {
                void openArticle(articleId).then(setSnapshot);
              }}
            />
            <ArticleGroup
              title="全部"
              articles={groupedArticles.all}
              selectedId={snapshot.selectedArticleId}
              onOpen={(articleId) => {
                void openArticle(articleId).then(setSnapshot);
              }}
            />
            <ArticleGroup
              title="收藏"
              articles={groupedArticles.favorite}
              selectedId={snapshot.selectedArticleId}
              onOpen={(articleId) => {
                void openArticle(articleId).then(setSnapshot);
              }}
            />
          </aside>

          <section className="article-detail-panel">
            <div className="pool-stat-grid">
              {snapshot.contentPoolStats.map((stat) => (
                <SourceStatCard key={stat.sourceKind} stat={stat} />
              ))}
            </div>

            {selectedArticle ? (
              <article className="article-detail">
                <div className="detail-head">
                  <div>
                    <p className="detail-source">
                      {selectedArticle.sourceName} · {DISCIPLINE_LABELS[selectedArticle.discipline]} ·{" "}
                      {SOURCE_KIND_LABELS[selectedArticle.sourceKind]}
                    </p>
                    <h2>{selectedArticle.title}</h2>
                  </div>
                  <button onClick={() => void toggleFavorite(selectedArticle.id).then(setSnapshot)}>
                    {selectedArticle.isFavorite ? "取消收藏" : "收藏"}
                  </button>
                </div>

                <div className="detail-highlight">
                  <strong>{selectedArticle.fitScore} 分</strong>
                  <span>{fitLabel(selectedArticle.fitLevel)} 契合度</span>
                </div>

                <dl className="detail-meta">
                  <div>
                    <dt>摘要</dt>
                    <dd>{selectedArticle.summary}</dd>
                  </div>
                  <div>
                    <dt>来源元数据</dt>
                    <dd>
                      {DISCIPLINE_LABELS[selectedArticle.discipline]} /{" "}
                      {SOURCE_KIND_LABELS[selectedArticle.sourceKind]} /{" "}
                      {RESOURCE_TYPE_LABELS[selectedArticle.resourceType]}
                    </dd>
                  </div>
                  <div>
                    <dt>链接</dt>
                    <dd>
                      <a href={selectedArticle.link} target="_blank" rel="noreferrer">
                        {selectedArticle.link}
                      </a>
                    </dd>
                  </div>
                  <div>
                    <dt>发布时间</dt>
                    <dd>{formatArticleTime(selectedArticle.publishedAt)}</dd>
                  </div>
                  <div>
                    <dt>抓取时间</dt>
                    <dd>{formatArticleTime(selectedArticle.fetchedAt)}</dd>
                  </div>
                  <div>
                    <dt>推荐理由</dt>
                    <dd>{selectedArticle.recommendationReason}</dd>
                  </div>
                </dl>
              </article>
            ) : (
              <div className="panel-empty">还没有可展示内容，等待调度和抓取结果。</div>
            )}
          </section>
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
