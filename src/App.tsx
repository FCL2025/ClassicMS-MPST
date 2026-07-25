import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import {
  initialDataDir,
  initialPath,
  listChildren,
  onProgress,
  openDataDir,
  pickDataDirectory,
  typeLabel,
  type ChildEntry,
  type ImportProgress,
  type OpenInfo,
  type OpenResult,
} from "./api";
import { ExportMenu } from "./components/ExportMenu";
import { ChangelogDialog } from "./components/ChangelogDialog";
import { NodeDetail } from "./components/NodeDetail";
import { SearchBar } from "./components/SearchBar";
import { TreeView } from "./components/TreeView";
import { useTheme } from "./useTheme";
import { version as APP_VERSION } from "../package.json";

const MIN_TREE_WIDTH = 240;
const DEFAULT_TREE_WIDTH = 380;

function initialTreeWidth(): number {
  const saved = Number(window.localStorage.getItem("msclassic.treePaneWidth"));
  return Number.isFinite(saved) && saved >= MIN_TREE_WIDTH ? saved : DEFAULT_TREE_WIDTH;
}

function findLoadedEntry(childrenMap: Map<string, ChildEntry[]>, path: string): ChildEntry | null {
  for (const children of childrenMap.values()) {
    const entry = children.find((candidate) => candidate.path === path);
    if (entry) return entry;
  }
  return null;
}

function selectedTypeLabel(entry: ChildEntry | null): string {
  if (!entry) return "";
  if (entry.kind === "Category") return "Folder";
  if (entry.kind === "Node") return typeLabel(entry.type_tag);
  switch (entry.class_id) {
    case 49:
      return "TextAsset";
    case 83:
      return "AudioClip";
    case 114:
      return "MonoBehaviour";
    default:
      return entry.class_id > 0 ? `Unity Class ${entry.class_id}` : "Asset";
  }
}

function statusFor(result: OpenResult): string {
  return (
    `${result.data_dir} — ${result.info.entry_count.toLocaleString()} 筆資產 / ` +
    `${result.info.bundle_count} bundles` +
    (result.info.elapsed_ms > 0
      ? `（索引 ${result.info.elapsed_ms}ms${result.info.from_cache ? "，快取命中" : "，重建"}）`
      : "")
  );
}

export default function App() {
  const { theme, toggleTheme } = useTheme();
  const [childrenMap, setChildrenMap] = useState<Map<string, ChildEntry[]>>(new Map());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<ChildEntry | null>(null);
  const [openInfo, setOpenInfo] = useState<OpenInfo | null>(null);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [dataError, setDataError] = useState<string | null>(null);
  const [loadingData, setLoadingData] = useState(false);
  const [status, setStatus] = useState("請先選擇楓之谷經典版的遊戲資料夾");
  const [toast, setToast] = useState<{ message: string; tone: "success" | "error" } | null>(null);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const [treeWidth, setTreeWidth] = useState(initialTreeWidth);
  const [resizingTree, setResizingTree] = useState(false);
  const [showChangelog, setShowChangelog] = useState(false);
  const treeWidthRef = useRef(treeWidth);
  const childrenMapRef = useRef(childrenMap);
  const childRequestsRef = useRef(new Map<string, Promise<void>>());
  const navigationGenerationRef = useRef(0);
  const datasetGenerationRef = useRef(0);
  const initialTargetRequestRef = useRef<Promise<string | null> | null>(null);

  const storeChildren = useCallback((path: string, children: ChildEntry[]) => {
    const next = new Map(childrenMapRef.current).set(path, children);
    childrenMapRef.current = next;
    setChildrenMap(next);
  }, []);

  const loadDataDirectory = useCallback(async (path: string) => {
    const generation = ++datasetGenerationRef.current;
    navigationGenerationRef.current += 1;
    childRequestsRef.current.clear();
    setLoadingData(true);
    setDataError(null);
    setStatus(`正在開啟 ${path}…`);

    try {
      const result = await openDataDir(path);
      if (generation !== datasetGenerationRef.current) return;
      const initialChildren = new Map([["", result.top]]);
      childrenMapRef.current = initialChildren;
      initialTargetRequestRef.current = null;
      setChildrenMap(initialChildren);
      setExpanded(new Set());
      setSelected(null);
      setOpenInfo(result.info);
      setDataDir(result.data_dir);
      setStatus(statusFor(result));
    } catch (error) {
      if (generation !== datasetGenerationRef.current) return;
      const message = String(error);
      setDataError(message);
      setStatus(`開啟失敗：${message}`);
    } finally {
      if (generation === datasetGenerationRef.current) setLoadingData(false);
    }
  }, []);

  const chooseDataDirectory = useCallback(async () => {
    try {
      const selectedPath = await pickDataDirectory();
      if (typeof selectedPath === "string") {
        await loadDataDirectory(selectedPath);
      }
    } catch (error) {
      const message = String(error);
      setDataError(message);
      setStatus(`無法開啟資料夾選擇器：${message}`);
    }
  }, [loadDataDirectory]);

  const childrenOf = useCallback((path: string) => childrenMap.get(path), [childrenMap]);

  const loadChildren = useCallback(
    (path: string) => {
      if (childrenMapRef.current.has(path)) return Promise.resolve();
      const pending = childRequestsRef.current.get(path);
      if (pending) return pending;
      const datasetGeneration = datasetGenerationRef.current;

      const request = listChildren(path)
        .then((children) => {
          if (datasetGeneration === datasetGenerationRef.current) {
            storeChildren(path, children);
          }
        })
        .finally(() => {
          childRequestsRef.current.delete(path);
        });
      childRequestsRef.current.set(path, request);
      return request;
    },
    [storeChildren],
  );

  const onToggle = useCallback(
    (path: string) => {
      setExpanded((previous) => {
        const next = new Set(previous);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
      void loadChildren(path).catch((error) => {
        setStatus(`載入 ${path} 失敗：${error}`);
      });
    },
    [loadChildren],
  );

  const onPickSearchResult = useCallback(
    (path: string) => {
      const generation = ++navigationGenerationRef.current;
      const segments = path.split("/");
      const prefixes = segments
        .slice(0, -1)
        .map((_, index) => segments.slice(0, index + 1).join("/"));

      return prefixes
        .reduce((pending, prefix) => pending.then(() => loadChildren(prefix)), Promise.resolve())
        .then(() => {
          if (generation !== navigationGenerationRef.current) return;
          setExpanded((previous) => new Set([...previous, ...prefixes]));
          const entry = findLoadedEntry(childrenMapRef.current, path);
          if (entry) {
            setSelected(entry);
          } else {
            setStatus(`找不到導覽目標：${path}`);
          }
        })
        .catch((error) => {
          if (generation === navigationGenerationRef.current) {
            setStatus(`導航至 ${path} 失敗：${error}`);
          }
        });
    },
    [loadChildren],
  );

  // 自動化測試用：設定 MSCLASSIC_DATA_DIR 時啟動即開啟該資料目錄。
  useEffect(() => {
    initialDataDir()
      .then((dir) => {
        if (dir) void loadDataDirectory(dir);
      })
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!openInfo) return;
    let active = true;
    if (!initialTargetRequestRef.current) {
      try {
        const hash = decodeURIComponent(window.location.hash.replace(/^#\/?/, ""));
        initialTargetRequestRef.current = hash ? Promise.resolve(hash) : initialPath();
      } catch {
        setStatus("初始路徑格式無效");
        return;
      }
    }
    initialTargetRequestRef.current
      .then((path) => {
        if (active && path) void onPickSearchResult(path);
      })
      .catch((error) => {
        if (active) setStatus(`讀取初始路徑失敗：${error}`);
      });
    return () => {
      active = false;
    };
  }, [openInfo, onPickSearchResult]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 5000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  // web 版才有匯入進度(OPFS 複製 / 索引建立);桌面版 onProgress 不會觸發。
  useEffect(() => onProgress(setImportProgress), []);

  const resizeTree = useCallback((clientX: number) => {
    const maxWidth = Math.max(MIN_TREE_WIDTH, window.innerWidth * 0.58);
    const nextWidth = Math.round(Math.min(maxWidth, Math.max(MIN_TREE_WIDTH, clientX)));
    treeWidthRef.current = nextWidth;
    setTreeWidth(nextWidth);
  }, []);

  const onResizeStart = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    setResizingTree(true);
    resizeTree(event.clientX);
  };

  const onResizeMove = (event: PointerEvent<HTMLDivElement>) => {
    if (resizingTree) resizeTree(event.clientX);
  };

  const finishResize = (event: PointerEvent<HTMLDivElement>) => {
    if (!resizingTree) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setResizingTree(false);
    window.localStorage.setItem("msclassic.treePaneWidth", String(treeWidthRef.current));
  };

  const onResizeKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const delta = event.key === "ArrowLeft" ? -16 : 16;
    const next = Math.min(
      Math.max(MIN_TREE_WIDTH, window.innerWidth * 0.58),
      Math.max(MIN_TREE_WIDTH, treeWidth + delta),
    );
    treeWidthRef.current = next;
    setTreeWidth(next);
    window.localStorage.setItem("msclassic.treePaneWidth", String(next));
  };

  const workspaceStyle = {
    "--tree-pane-width": `${treeWidth}px`,
  } as CSSProperties;

  const selectedIsCategory = selected?.kind === "Category";
  const detailPath = selected && !selectedIsCategory ? selected.path : null;

  return (
    <div className="app">
      <header className="app-header">
        <ExportMenu
          canExportPng={
            selected?.kind === "Node" &&
            (selected.type_tag === 18 ||
              selected.type_tag === 19 ||
              selected.type_tag === 12)
          }
          nodeName={selected?.name ?? null}
          path={detailPath}
          onMessage={(message, tone = "success") => {
            setToast({ message, tone });
            setStatus(message);
          }}
          onShowChangelog={() => setShowChangelog(true)}
        />
        {openInfo ? (
          <div className="header-summary" title={dataDir ?? undefined}>
            {openInfo.entry_count.toLocaleString()} 資產 · {openInfo.bundle_count} bundles
          </div>
        ) : (
          <div className="header-summary">尚未選擇資料來源</div>
        )}
        <button
          className="data-folder-button"
          type="button"
          disabled={loadingData}
          onClick={() => void chooseDataDirectory()}
        >
          <span aria-hidden="true">▣</span>
          {loadingData ? "載入中…" : openInfo ? "更換資料夾" : "選擇資料夾"}
        </button>
        <button
          className="icon-button"
          type="button"
          onClick={toggleTheme}
          title={theme === "dark" ? "切換為淺色" : "切換為深色"}
          aria-label="切換主題"
        >
          {theme === "dark" ? "☀" : "☾"}
        </button>
      </header>

      {importProgress ? (
        <div className="import-progress" role="status">
          <span className="import-progress-message">{importProgress.message}</span>
          <div
            className="import-progress-track"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(importProgress.percent)}
          >
            <div
              className="import-progress-bar"
              style={{ width: `${Math.min(100, Math.max(0, importProgress.percent))}%` }}
            />
          </div>
          <span className="import-progress-percent">
            {Math.round(importProgress.percent)}%
          </span>
        </div>
      ) : null}

      {openInfo ? (
        <div className="app-body" style={workspaceStyle}>
          <aside className="app-left" aria-label="資產樹">
            <TreeView
              childrenOf={childrenOf}
              expanded={expanded}
              selected={selected?.path ?? null}
              onToggle={onToggle}
              onSelect={setSelected}
            />
            <div className="tree-panel-heading">
              <SearchBar onPick={onPickSearchResult} />
            </div>
          </aside>
          <div
            className={`workspace-resizer${resizingTree ? " is-active" : ""}`}
            role="separator"
            tabIndex={0}
            aria-label="調整資產樹寬度"
            aria-orientation="vertical"
            aria-valuemin={MIN_TREE_WIDTH}
            aria-valuemax={Math.round(window.innerWidth * 0.58)}
            aria-valuenow={treeWidth}
            onKeyDown={onResizeKeyDown}
            onPointerDown={onResizeStart}
            onPointerMove={onResizeMove}
            onPointerUp={finishResize}
            onPointerCancel={finishResize}
          >
            <span />
          </div>
          <main className="app-right">
            <NodeDetail path={detailPath} emptyMode={selectedIsCategory ? "category" : "selection"} />
          </main>
        </div>
      ) : (
        <main className="data-source-welcome">
          <section>
            <p className="welcome-eyebrow">開始使用</p>
            <h1>選擇遊戲資料夾</h1>
            <p className="welcome-description">
              請選擇楓之谷經典版的安裝目錄、Maplestory_Classic_Data，
              或直接選擇包含 bundle 的 StreamingAssets\aa\w。
            </p>
            <button
              className="welcome-open-button"
              type="button"
              disabled={loadingData}
              onClick={() => void chooseDataDirectory()}
            >
              {loadingData ? "正在建立資產索引…" : "選擇資料夾"}
            </button>
            {dataError ? <p className="data-source-error" role="alert">{dataError}</p> : null}
            <p className="welcome-hint">程式只會讀取遊戲資料，不會修改原始 bundle。</p>
            {__WEB_BUILD__ ? (
              <p className="welcome-hint">
                瀏覽器的資料夾對話框若顯示「上傳」字樣屬正常現象（瀏覽器固定文案），
                檔案僅在你的電腦本機讀取，不會上傳到任何伺服器。
              </p>
            ) : null}
          </section>
        </main>
      )}

      {toast ? (
        <div className={`app-toast is-${toast.tone}`} role="status">
          {toast.message}
        </div>
      ) : null}
      <div className="sr-only" role="status" aria-live="polite">{status}</div>
      <ChangelogDialog open={showChangelog} onClose={() => setShowChangelog(false)} />
      <footer className="app-footer" title={selected?.path ?? undefined}>
        <span className="footer-node-type">
          {selected ? `類型：${selectedTypeLabel(selected)}` : ""}
        </span>
        <span className="footer-node-path">{selected?.path ?? ""}</span>
        <span className="footer-version">v{APP_VERSION}</span>
      </footer>
    </div>
  );
}
