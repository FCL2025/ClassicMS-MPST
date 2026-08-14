import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import { Cloud, Folder, FolderSync, Moon, Sun } from "lucide-react";
import {
  getAudio,
  getDataSource,
  initialDataDir,
  initialPath,
  initialView,
  listChildren,
  onProgress,
  openDataDir,
  pickDataDirectory,
  reconnectDataDirectory,
  setDataSource,
  storedFolderLabel,
  supportsDataSourceSwitch,
  typeLabel,
  type ChildEntry,
  type DataSource,
  type ImportProgress,
  type OpenInfo,
  type OpenResult,
} from "./api";
import { AvatarPage } from "./components/AvatarPage";
import { PetAvatarPage } from "./components/PetAvatarPage";
import { ExportMenu } from "./components/ExportMenu";
import { ChangelogDialog } from "./components/ChangelogDialog";
import { MonsterBookPage } from "./components/MonsterBookPage";
import { SkillBookPage } from "./components/SkillBookPage";
import { QuestPage } from "./components/QuestPage";
import { NodeDetail } from "./components/NodeDetail";
import { SearchBar } from "./components/SearchBar";
import { TreeView } from "./components/TreeView";
import { useTheme } from "./useTheme";
import { version as APP_VERSION } from "../package.json";

const MIN_TREE_WIDTH = 240;
const DEFAULT_TREE_WIDTH = 380;

// 資料來源切換（hosted 版限定）：true 時 header 顯示「雲端資料 / 本機資料夾」切換器。
const CAN_SWITCH_DATA_SOURCE = supportsDataSourceSwitch();
// 切到雲端時 loadDataDirectory 的 path 參數僅用於狀態列訊息（hosted openDataDir 會忽略）。
const CLOUD_SOURCE_LABEL = "雲端資料";

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
  // 資料來源（hosted 版才有切換器；偏好存 localStorage，預設本機資料夾）。
  const [dataSource, setDataSourceState] = useState<DataSource>(() => getDataSource());
  const [, setDataDir] = useState<string | null>(null);
  const [dataError, setDataError] = useState<string | null>(null);
  const [loadingData, setLoadingData] = useState(false);
  const [status, setStatus] = useState("請先選擇楓之谷經典版的遊戲資料夾");
  const [toast, setToast] = useState<{ message: string; tone: "success" | "error" } | null>(null);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const [treeWidth, setTreeWidth] = useState(initialTreeWidth);
  const [resizingTree, setResizingTree] = useState(false);
  const [showChangelog, setShowChangelog] = useState(false);
  // 頂部 view 切換：瀏覽（資產樹）/ 紙娃娃（角色試穿）/ 寵物（寵物試穿）/ 怪物圖鑑 / 技能 / 任務。
  const [view, setView] = useState<"tree" | "avatar" | "pet" | "monster" | "skill" | "quest">(
    "tree",
  );
  // 紙娃娃第一次切過去才掛載（meta/equipment 此時才載入），之後保持掛載不重置狀態。
  const [avatarActivated, setAvatarActivated] = useState(false);
  // 寵物紙娃娃同樣 lazy-mount（第一次切過去才載入寵物/寵裝清單）。
  const [petAvatarActivated, setPetAvatarActivated] = useState(false);
  // 怪物圖鑑同樣 lazy-mount（第一次切過去才載入圖鑑清單）。
  const [monsterActivated, setMonsterActivated] = useState(false);
  // 技能瀏覽器同樣 lazy-mount（第一次切過去才載入技能書）。
  const [skillActivated, setSkillActivated] = useState(false);
  // 任務資料庫同樣 lazy-mount（第一次切過去才載入任務清單）。
  const [questActivated, setQuestActivated] = useState(false);
  // 紙娃娃商店的「點裝」開關：ON 時只列 cash 裝備（App 持有，切 view 不重置）。
  const [cashOnly, setCashOnly] = useState(false);
  // 紙娃娃 BGM：切到紙娃娃 tab 時循環播放 Sound/BgmUI/ShopBgm；false 表示使用者手動暫停。
  const [bgmOn, setBgmOn] = useState(true);
  // 紙娃娃 BGM 音量（0～1），拉桿即時調整，不需重播音訊。
  const [bgmVolume, setBgmVolume] = useState(0.5);
  const bgmAudioRef = useRef<HTMLAudioElement | null>(null);
  const bgmUrlRef = useRef<string | null>(null);
  const bgmVolumeRef = useRef(bgmVolume);
  // 紙娃娃/寵物頁最後點選的裝備 img 路徑（footer 麵包屑用）。
  const [avatarPick, setAvatarPick] = useState<string | null>(null);
  // 怪物圖鑑最後選取的圖鑑路徑（footer 麵包屑用）。
  const [mobPick, setMobPick] = useState<string | null>(null);
  // 任務頁掉落來源彈窗點怪物時的跨頁跳轉請求（nonce 讓同 id 可重複觸發）。
  const [mobJump, setMobJump] = useState<{ mobId: number; nonce: number } | null>(null);
  // 技能瀏覽器最後選取的技能路徑（footer 麵包屑用）。
  const [skillPick, setSkillPick] = useState<string | null>(null);
  // 任務資料庫最後選取的任務路徑（footer 麵包屑用）。
  const [questPick, setQuestPick] = useState<string | null>(null);
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

  // 切到怪物圖鑑並選取指定怪物（任務頁掉落來源彈窗的跳轉）。
  const openMobInBook = useCallback((mobId: number) => {
    setView("monster");
    setMonsterActivated(true);
    setMobJump({ mobId, nonce: Date.now() });
  }, []);

  const clearMobJump = useCallback(() => setMobJump(null), []);

  // 紙娃娃 BGM：view 切到 avatar 且未手動暫停時播放（單曲循環），離開或暫停時停下。
  // AudioClip 內容是 Ogg Vorbis；blob URL 與 <audio> 元素只做一次，暫停會保留播放位置。
  useEffect(() => {
    if (view !== "avatar" || !bgmOn) {
      bgmAudioRef.current?.pause();
      return;
    }
    let cancelled = false;
    const play = async () => {
      try {
        if (!bgmUrlRef.current) {
          const result = await getAudio("Sound/BgmUI/ShopBgm");
          const bytes = Uint8Array.from(atob(result.base64), (c) => c.charCodeAt(0));
          bgmUrlRef.current = URL.createObjectURL(new Blob([bytes], { type: "audio/ogg" }));
        }
        if (cancelled) return;
        if (!bgmAudioRef.current) {
          const audio = new Audio(bgmUrlRef.current);
          audio.loop = true;
          audio.volume = bgmVolumeRef.current;
          bgmAudioRef.current = audio;
        }
        await bgmAudioRef.current.play();
      } catch (error) {
        if (!cancelled) setStatus(`背景音樂播放失敗：${String(error)}`);
      }
    };
    void play();
    return () => {
      cancelled = true;
    };
  }, [view, bgmOn]);

  // 音量拉桿即時套用到已建立的 <audio>；元素尚未建立時只更新 ref，建立時再讀取。
  useEffect(() => {
    bgmVolumeRef.current = bgmVolume;
    const audio = bgmAudioRef.current;
    if (audio) audio.volume = bgmVolume;
  }, [bgmVolume]);

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

  // web 版「記住資料夾」：有記錄時歡迎頁提供一鍵重新連線（桌面版恆 null 不顯示）。
  const [reconnectLabel, setReconnectLabel] = useState<string | null>(null);

  const reconnectStoredFolder = useCallback(async () => {
    try {
      const label = await reconnectDataDirectory();
      if (typeof label === "string") {
        setReconnectLabel(label);
        await loadDataDirectory(label);
      }
    } catch (error) {
      const message = String(error);
      setDataError(message);
      setStatus(`重新連線失敗：${message}`);
    }
  }, [loadDataDirectory]);

  // 來源切換器（hosted 版）：切到雲端 = 重新載入託管資料集。
  const switchToCloud = useCallback(async () => {
    if (dataSource === "cloud" || loadingData) return;
    setDataSource("cloud");
    setDataSourceState("cloud");
    await loadDataDirectory(CLOUD_SOURCE_LABEL);
  }, [dataSource, loadingData, loadDataDirectory]);

  // 來源切換器（hosted 版）：切到本機 = 有記住的資料夾則重連，否則跳選擇器；
  // 使用者取消選擇時回復雲端模式，避免切過去卻沒資料的空狀態。
  const switchToLocal = useCallback(async () => {
    if (dataSource === "local" || loadingData) return;
    // 先切模式，api 委派層才會把 reconnect/pick 打到本機後端。
    setDataSource("local");
    try {
      const label = (await reconnectDataDirectory()) ?? (await pickDataDirectory());
      if (!label) {
        setDataSource("cloud");
        return;
      }
      setDataSourceState("local");
      await loadDataDirectory(label);
    } catch (error) {
      setDataSource("cloud");
      const message = String(error);
      setDataError(message);
      setStatus(`切換本機資料夾失敗：${message}`);
    }
  }, [dataSource, loadingData, loadDataDirectory]);

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
  // web 版則由 initialDataDir 改回傳 OPFS 快取 / 記住的資料夾標籤，達成啟動自動載入。
  useEffect(() => {
    initialDataDir()
      .then((dir) => {
        if (dir) void loadDataDirectory(dir);
      })
      .catch(() => undefined);
    // web 版「記住資料夾」：有記錄時在歡迎頁顯示一鍵重新連線。
    storedFolderLabel()
      .then((label) => setReconnectLabel(label))
      .catch(() => undefined);
    // 自動化測試用：MSCLASSIC_INITIAL_VIEW 可指定啟動 view（tree/avatar/pet/monster/skill/quest）。
    initialView()
      .then((target) => {
        if (target === "avatar") {
          setView("avatar");
          setAvatarActivated(true);
        } else if (target === "pet") {
          setView("pet");
          setPetAvatarActivated(true);
        } else if (target === "monster") {
          setView("monster");
          setMonsterActivated(true);
        } else if (target === "skill") {
          setView("skill");
          setSkillActivated(true);
        } else if (target === "quest") {
          setView("quest");
          setQuestActivated(true);
        }
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
        <div className="view-tabs" role="tablist" aria-label="檢視切換">
          <button
            type="button"
            role="tab"
            aria-selected={view === "tree"}
            className={view === "tree" ? "is-active" : ""}
            onClick={() => setView("tree")}
          >
            瀏覽
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={view === "avatar"}
            className={view === "avatar" ? "is-active" : ""}
            disabled={!openInfo}
            title={openInfo ? "角色試穿" : "請先選擇資料夾"}
            onClick={() => {
              setView("avatar");
              setAvatarActivated(true);
            }}
          >
            紙娃娃
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={view === "pet"}
            className={view === "pet" ? "is-active" : ""}
            disabled={!openInfo}
            title={openInfo ? "寵物試穿" : "請先選擇資料夾"}
            onClick={() => {
              setView("pet");
              setPetAvatarActivated(true);
            }}
          >
            寵物
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={view === "monster"}
            className={view === "monster" ? "is-active" : ""}
            disabled={!openInfo}
            title={openInfo ? "怪物圖鑑" : "請先選擇資料夾"}
            onClick={() => {
              setView("monster");
              setMonsterActivated(true);
            }}
          >
            怪物圖鑑
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={view === "skill"}
            className={view === "skill" ? "is-active" : ""}
            disabled={!openInfo}
            title={openInfo ? "技能瀏覽器" : "請先選擇資料夾"}
            onClick={() => {
              setView("skill");
              setSkillActivated(true);
            }}
          >
            技能
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={view === "quest"}
            className={view === "quest" ? "is-active" : ""}
            disabled={!openInfo}
            title={openInfo ? "任務資料庫" : "請先選擇資料夾"}
            onClick={() => {
              setView("quest");
              setQuestActivated(true);
            }}
          >
            任務
          </button>
        </div>
        <div className="header-divider" aria-hidden="true" />
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
        <div className="header-spacer" />
        {view === "avatar" ? (
          <>
            <button
              className="icon-button bgm-button"
              type="button"
              onClick={() => setBgmOn((on) => !on)}
              title={bgmOn ? "暫停背景音樂" : "播放背景音樂"}
              aria-label={bgmOn ? "暫停背景音樂" : "播放背景音樂"}
            >
              {bgmOn ? "⏸" : "▶"}
            </button>
            <input
              className="bgm-volume"
              type="range"
              min={0}
              max={100}
              value={Math.round(bgmVolume * 100)}
              onChange={(event) => setBgmVolume(Number(event.target.value) / 100)}
              title={`背景音樂音量：${Math.round(bgmVolume * 100)}%`}
              aria-label="背景音樂音量"
            />
            <label className="cash-switch" title="只顯示點數裝備">
              <input
                type="checkbox"
                checked={cashOnly}
                onChange={(event) => setCashOnly(event.target.checked)}
              />
              <span className="cash-switch-track" aria-hidden="true">
                <span className="cash-switch-thumb" />
              </span>
              點裝
            </label>
          </>
        ) : null}
        {CAN_SWITCH_DATA_SOURCE ? (
          <>
            <div className="source-switch" role="tablist" aria-label="資料來源">
              <button
                type="button"
                role="tab"
                aria-selected={dataSource === "cloud"}
                className={dataSource === "cloud" ? "is-active" : ""}
                disabled={loadingData}
                title="Cloudflare 託管的預處理資料，免安裝遊戲"
                onClick={() => void switchToCloud()}
              >
                <Cloud aria-hidden="true" size={15} strokeWidth={2.25} />
                雲端資料
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={dataSource === "local"}
                className={dataSource === "local" ? "is-active" : ""}
                disabled={loadingData}
                title="讀取自己電腦裡的遊戲資料夾"
                onClick={() => void switchToLocal()}
              >
                <Folder aria-hidden="true" size={15} strokeWidth={2.25} />
                本機資料夾
              </button>
            </div>
            {dataSource === "local" ? (
              <button
                className="data-folder-button"
                type="button"
                disabled={loadingData}
                onClick={() => void chooseDataDirectory()}
              >
                {openInfo ? (
                  <FolderSync aria-hidden="true" size={15} strokeWidth={2.25} />
                ) : (
                  <Folder aria-hidden="true" size={15} strokeWidth={2.25} />
                )}
                {loadingData ? "載入中…" : openInfo ? "更換資料夾" : "選擇資料夾"}
              </button>
            ) : null}
          </>
        ) : (
          <button
            className="data-folder-button"
            type="button"
            disabled={loadingData}
            onClick={() => void chooseDataDirectory()}
          >
            {openInfo ? (
              <FolderSync aria-hidden="true" size={15} strokeWidth={2.25} />
            ) : (
              <Folder aria-hidden="true" size={15} strokeWidth={2.25} />
            )}
            {loadingData ? "載入中…" : openInfo ? "更換資料夾" : "選擇資料夾"}
          </button>
        )}
        <button
          className="icon-button"
          type="button"
          onClick={toggleTheme}
          title={theme === "dark" ? "切換為淺色" : "切換為深色"}
          aria-label="切換主題"
        >
                   {theme === "dark" ? (
            <Sun aria-hidden="true" size={15} strokeWidth={2.25} />
          ) : (
            <Moon aria-hidden="true" size={15} strokeWidth={2.25} />
          )}
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
        <div
          className={`app-body${view === "avatar" ? " is-avatar" : ""}${view === "pet" ? " is-pet" : ""}${view === "monster" ? " is-monster" : ""}${view === "skill" ? " is-skill" : ""}${view === "quest" ? " is-quest" : ""}`}
          style={workspaceStyle}
        >
          {view === "tree" ? (
            <>
              <aside className="app-left" aria-label="資產樹">
                <TreeView
                  childrenOf={childrenOf}
                  expanded={expanded}
                  selected={selected?.path ?? null}
                  onToggle={onToggle}
                  onSelect={setSelected}
                />
                <div className="tree-panel-heading">
                  <SearchBar
                    scope={selected?.path ?? null}
                    onClearScope={() => setSelected(null)}
                    onPick={onPickSearchResult}
                  />
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
            </>
          ) : null}
          <main className="app-right">
            {view === "tree" ? (
              <NodeDetail
                path={detailPath}
                emptyMode={selectedIsCategory ? "category" : "selection"}
              />
            ) : null}
            {avatarActivated || view === "avatar" ? (
              <AvatarPage
                visible={view === "avatar"}
                cashOnly={cashOnly}
                onPickItem={setAvatarPick}
              />
            ) : null}
            {petAvatarActivated || view === "pet" ? (
              <PetAvatarPage visible={view === "pet"} onPickItem={setAvatarPick} />
            ) : null}
            {monsterActivated || view === "monster" ? (
              <MonsterBookPage
                visible={view === "monster"}
                onPickMob={setMobPick}
                jumpToMob={mobJump}
                onJumpToMobHandled={clearMobJump}
              />
            ) : null}
            {skillActivated || view === "skill" ? (
              <SkillBookPage visible={view === "skill"} onPickSkill={setSkillPick} />
            ) : null}
            {questActivated || view === "quest" ? (
              <QuestPage
                visible={view === "quest"}
                onPickQuest={setQuestPick}
                onOpenMob={openMobInBook}
              />
            ) : null}
          </main>
        </div>
      ) : (
        <main className="data-source-welcome">
          <section>
            {CAN_SWITCH_DATA_SOURCE && dataSource === "cloud" ? (
              <>
                <p className="welcome-eyebrow">開始使用</p>
                <h1>連線雲端資料</h1>
                <p className="welcome-description">
                  正在連線 Cloudflare 託管的遊戲資料。若連線失敗，請確認網路後重試，
                  或用右上角的來源切換器改讀本機資料夾。
                </p>
                <button
                  className="welcome-open-button"
                  type="button"
                  disabled={loadingData}
                  onClick={() => void loadDataDirectory(CLOUD_SOURCE_LABEL)}
                >
                  {loadingData ? "正在連線雲端資料…" : "重試連線雲端資料"}
                </button>
                {dataError ? <p className="data-source-error" role="alert">{dataError}</p> : null}
              </>
            ) : (
              <>
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
                {reconnectLabel ? (
                  <button
                    className="welcome-reconnect-button"
                    type="button"
                    disabled={loadingData}
                    onClick={() => void reconnectStoredFolder()}
                  >
                    重新連線上次資料夾：{reconnectLabel}
                  </button>
                ) : null}
                {dataError ? <p className="data-source-error" role="alert">{dataError}</p> : null}
                <p className="welcome-hint">程式只會讀取遊戲資料，不會修改原始 bundle。</p>
                {__WEB_BUILD__ ? (
                  <p className="welcome-hint">
                    瀏覽器的資料夾對話框若顯示「上傳」字樣屬正常現象（瀏覽器固定文案），
                    檔案僅在你的電腦本機讀取，不會上傳到任何伺服器。
                  </p>
                ) : null}
                {__WEB_BUILD__ && !CAN_SWITCH_DATA_SOURCE ? (
                  <p className="welcome-hint">
                    Chrome／Edge 若提示「無法開啟此資料夾，因為其包含系統檔案」，直接取消該對話框，
                    再按一次「選擇資料夾」即會改用相容選擇器，同樣的位置即可正常選取。
                  </p>
                ) : null}
              </>
            )}
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
      <footer
        className="app-footer"
        title={
          view === "avatar" || view === "pet"
            ? (avatarPick ?? undefined)
            : view === "monster"
              ? (mobPick ?? undefined)
              : view === "skill"
                ? (skillPick ?? undefined)
                : view === "quest"
                  ? (questPick ?? undefined)
                  : (selected?.path ?? undefined)
        }
      >
        {view === "avatar" || view === "pet" ? (
          <>
            <span className="footer-node-type">{avatarPick ? "類型：img" : ""}</span>
            <span className="footer-node-path">{avatarPick ?? ""}</span>
          </>
        ) : view === "monster" ? (
          <>
            <span className="footer-node-type">{mobPick ? "類型：img" : ""}</span>
            <span className="footer-node-path">{mobPick ?? ""}</span>
          </>
        ) : view === "skill" ? (
          <>
            <span className="footer-node-type">{skillPick ? "類型：img" : ""}</span>
            <span className="footer-node-path">{skillPick ?? ""}</span>
          </>
        ) : view === "quest" ? (
          <>
            <span className="footer-node-type">{questPick ? "類型：img" : ""}</span>
            <span className="footer-node-path">{questPick ?? ""}</span>
          </>
        ) : (
          <>
            <span className="footer-node-type">
              {selected ? `類型：${selectedTypeLabel(selected)}` : ""}
            </span>
            <span className="footer-node-path">{selected?.path ?? ""}</span>
          </>
        )}
        <span className="footer-version">v{APP_VERSION}</span>
      </footer>
    </div>
  );
}
