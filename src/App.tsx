import { useEffect, useMemo, useRef, useState } from "react";
import { appWindow } from "@tauri-apps/api/window";
import {
  bootstrap,
  openArticle,
  petDoubleClick,
  saveSettings,
  setActiveView,
  toggleFavorite
} from "./api";
import type { Article, FitLevel, Snapshot } from "./types";

const PET_STATUS_LABELS = {
  loading: "加载中",
  "needs-config": "待配置",
  scanning: "扫描中",
  idle: "待命中",
  "new-info": "新提醒"
} as const;

const PET_STATUS_HINTS = {
  loading: "",
  "needs-config": "双击去设置",
  scanning: "扫描中",
  idle: "",
  "new-info": "有新内容"
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
          <span className="article-card-subtitle">{article.sourceName}</span>
          <span className="article-card-meta">
            <span>{article.fitScore} 分</span>
            {article.isFavorite && <span>已收藏</span>}
          </span>
        </button>
      ))}
    </section>
  );
}

function MainWindow({
  snapshot,
  setSnapshot,
  loading,
  error
}: {
  snapshot: Snapshot | null;
  setSnapshot: (snapshot: Snapshot) => void;
  loading: boolean;
  error: string | null;
}) {
  const [saving, setSaving] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const grouped = useMemo(
    () => ({
      fresh: (snapshot?.articles ?? []).filter((article) => article.isNew),
      all: snapshot?.articles ?? [],
      favorite: (snapshot?.articles ?? []).filter((article) => article.isFavorite)
    }),
    [snapshot?.articles]
  );

  const selectedArticle =
    snapshot?.articles.find((article) => article.id === snapshot.selectedArticleId) ?? null;

  async function handleSave(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!snapshot) {
      return;
    }

    const formData = new FormData(event.currentTarget);
    const rssSources = snapshot.settings.rssSources.map((source) => ({
      ...source,
      enabled: formData.get(source.id) === "on"
    }));

    setSaving(true);
    setSubmitError(null);
    try {
      const next = await saveSettings({
        apiKey: String(formData.get("apiKey") ?? ""),
        interestProfile: String(formData.get("interestProfile") ?? ""),
        autoStart: formData.get("autoStart") === "on",
        rssSources
      });
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
          <p>桌面上的信息提醒伙伴</p>
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
          <strong>{snapshot?.activeReminder ? `${snapshot.activeReminder.articleCount} 条待提醒` : "当前无提醒"}</strong>
        </div>
        <div className="overview-card">
          <span className="overview-label">API Key</span>
          <strong>{snapshot?.apiKeyValid ? "已验证可用" : "待验证或无效"}</strong>
        </div>
        <div className="overview-card">
          <span className="overview-label">最近抓取</span>
          <strong>{formatTime(snapshot?.lastScanAt ?? null)}</strong>
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
              <h2>基础配置</h2>
              <p>先提供有效 API Key，再让 Briefy-pet 进入抓取和分析流程。</p>
            </div>
            <label>
              <span>我的兴趣偏好</span>
              <textarea
                name="interestProfile"
                defaultValue={snapshot.settings.interestProfile}
                placeholder="例如：AI 产品、效率工具、前端工程、独立开发"
              />
            </label>
            <label>
              <span>API Key 设置</span>
              <input
                name="apiKey"
                type="password"
                defaultValue={snapshot.settings.apiKey}
                placeholder="输入你的 API Key"
              />
            </label>
          </section>

          <section className="settings-card">
            <div className="settings-section-head">
              <h2>内置 RSS 源</h2>
              <p>第一版只支持启用或禁用内置源，不支持自定义新增。</p>
            </div>
            <div className="rss-list">
              {snapshot.settings.rssSources.map((source) => (
                <label key={source.id} className="rss-item">
                  <input name={source.id} type="checkbox" defaultChecked={source.enabled} />
                  <div>
                    <strong>{source.name}</strong>
                    <span>{source.url}</span>
                  </div>
                </label>
              ))}
            </div>
            <label className="checkbox-row">
              <input name="autoStart" type="checkbox" defaultChecked={snapshot.settings.autoStart} />
              <span>开机启动</span>
            </label>
          </section>

          <div className="settings-actions">
            <button type="submit" disabled={saving}>
              {saving ? "验证并保存中..." : "保存设置"}
            </button>
          </div>
        </form>
      )}

      {!loading && !error && snapshot?.activeView === "reading" && (
        <div className="reading-view">
          <aside className="article-list-panel">
            <ArticleGroup
              title="新内容"
              articles={grouped.fresh}
              selectedId={snapshot.selectedArticleId}
              onOpen={(articleId) => {
                void openArticle(articleId).then(setSnapshot);
              }}
            />
            <ArticleGroup
              title="全部"
              articles={grouped.all}
              selectedId={snapshot.selectedArticleId}
              onOpen={(articleId) => {
                void openArticle(articleId).then(setSnapshot);
              }}
            />
            <ArticleGroup
              title="收藏"
              articles={grouped.favorite}
              selectedId={snapshot.selectedArticleId}
              onOpen={(articleId) => {
                void openArticle(articleId).then(setSnapshot);
              }}
            />
          </aside>

          <section className="article-detail-panel">
            {selectedArticle ? (
              <article className="article-detail">
                <div className="detail-head">
                  <div>
                    <p className="detail-source">{selectedArticle.sourceName}</p>
                    <h2>{selectedArticle.title}</h2>
                  </div>
                  <button onClick={() => void toggleFavorite(selectedArticle.id).then(setSnapshot)}>
                    {selectedArticle.isFavorite ? "取消收藏" : "收藏"}
                  </button>
                </div>

                <div className="detail-highlight">
                  <strong>{selectedArticle.fitScore} 分</strong>
                </div>

                <dl className="detail-meta">
                  <div>
                    <dt>摘要</dt>
                    <dd>{selectedArticle.summary}</dd>
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
                    <dt>来源</dt>
                    <dd>{selectedArticle.sourceName}</dd>
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
                    <dt>LLM 推荐理由</dt>
                    <dd>{selectedArticle.recommendationReason}</dd>
                  </div>
                </dl>
              </article>
            ) : (
              <div className="panel-empty">还没有可展示内容，等待抓取结果。</div>
            )}
          </section>
        </div>
      )}
    </div>
  );
}

export default function App() {
  const [windowLabel, setWindowLabel] = useState(() => appWindow.label);
  const pollIntervalMs = windowLabel === "main" ? 1500 : 400;
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

  return (
    <MainWindow
      snapshot={snapshot}
      setSnapshot={setSnapshot}
      loading={loading}
      error={error}
    />
  );
}
