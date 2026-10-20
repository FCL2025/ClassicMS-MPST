import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { getImage, type AvatarFrames } from "../api";
import {
  buildSequence,
  composeFrame,
  computeBoundingBox,
  exportCanvasPng,
  exportGif,
  exportWebP,
  frameIndexAt,
} from "./avatarExport";

// 貼圖快取（module-level）：試穿過程會反覆重用同一批 body/hair 貼圖，
// 整個 session 共用一份，換裝備只補抓新圖。
const imageCache = new Map<string, Promise<HTMLImageElement | null>>();

function decodeImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

function loadLayerImage(path: string): Promise<HTMLImageElement | null> {
  let cached = imageCache.get(path);
  if (!cached) {
    cached = (async () => {
      try {
        const result = await getImage(path);
        // 紙娃娃圖層必須是單張貼圖；缺少索引時的整頁預覽不能拿來合成角色。
        if (!result || result.mode !== "exact") return null;
        return await decodeImage(`data:image/png;base64,${result.base64}`);
      } catch {
        return null;
      }
    })();
    imageCache.set(path, cached);
  }
  return cached;
}

interface AvatarPreviewProps {
  frames: AvatarFrames | null;
  loading: boolean;
  error: string | null;
  playing: boolean;
  /** 紙娃娃 view 是否正在前景；切回瀏覽時停止 rAF。 */
  active: boolean;
  /** 目前動作名（匯出檔名用）。 */
  action: string;
}

export interface AvatarPreviewHandle {
  captureThumbnail: () => string | null;
}

// 正方形 canvas 預覽：棋盤格背景（CSS），body / face / ItemEff 特效三條動畫各自循環再疊加。
export const AvatarPreview = forwardRef<AvatarPreviewHandle, AvatarPreviewProps>(function AvatarPreview(
  { frames, loading, error, playing, active, action },
  ref,
) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [imageSet, setImageSet] = useState<{
    frames: AvatarFrames;
    images: Map<string, HTMLImageElement>;
  } | null>(null);
  const images = imageSet?.frames === frames ? imageSet.images : null;
  const [imagesError, setImagesError] = useState<string | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  // 目前動畫時間：暫停時凍結在這一幀，再播放從這裡繼續。
  const timeRef = useRef(0);
  const [exportDialog, setExportDialog] = useState(false);
  const [exporting, setExporting] = useState<"gif" | "webp" | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  // 追蹤容器尺寸（左欄寬度會隨視窗變）。
  useEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const measure = () => {
      const next = { width: wrap.clientWidth, height: wrap.clientHeight };
      setSize((previous) =>
        previous.width === next.width && previous.height === next.height ? previous : next,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(wrap);
    return () => observer.disconnect();
  }, []);

  // 收到新幀資料後，收齊所有 layer 貼圖才開始播（generation guard 防舊回應覆蓋）。
  useEffect(() => {
    if (!frames) {
      setImageSet(null);
      return;
    }
    let cancelled = false;
    setImageSet(null);
    setImagesError(null);
    // 新動畫從頭播。
    timeRef.current = 0;
    const paths = new Set<string>();
    for (const frame of [...frames.frames, ...frames.faceFrames, ...frames.effectFrames]) {
      for (const layer of frame.layers) paths.add(layer.path);
    }
    Promise.all(
      [...paths].map(async (path) => [path, await loadLayerImage(path)] as const),
    )
      .then((entries) => {
        if (cancelled) return;
        const next = new Map<string, HTMLImageElement>();
        for (const [path, img] of entries) {
          if (img) next.set(path, img);
        }
        setImageSet({ frames, images: next });
      })
      .catch((nextError) => {
        if (!cancelled) setImagesError(String(nextError));
      });
    return () => {
      cancelled = true;
    };
  }, [frames]);

  useImperativeHandle(ref, () => ({
    captureThumbnail: () => {
      if (!frames || !images || loading || error || imagesError) return null;
      const box = computeBoundingBox(frames, images);
      const body = buildSequence(frames.frames);
      if (!box || !body) return null;
      const thumbnail = document.createElement("canvas");
      thumbnail.width = 160;
      thumbnail.height = 160;
      const ctx = thumbnail.getContext("2d");
      if (!ctx) return null;
      const face = buildSequence(frames.faceFrames);
      const effect = buildSequence(frames.effectFrames);
      const fit = Math.min((160 - 16) / box.width, (160 - 16) / box.height);
      const scale = fit >= 1 ? Math.max(1, Math.floor(fit)) : fit;
      composeFrame(
        ctx,
        frames,
        images,
        frameIndexAt(body, timeRef.current),
        face ? frameIndexAt(face, timeRef.current) : -1,
        effect ? frameIndexAt(effect, timeRef.current) : -1,
        Math.round((160 - box.width * scale) / 2 - box.minX * scale),
        Math.round((160 - box.height * scale) / 2 - box.minY * scale),
        scale,
      );
      return thumbnail.toDataURL("image/png");
    },
  }), [frames, images, loading, error, imagesError]);

  const bodySequence = buildSequence(frames?.frames);
  const faceSequence = buildSequence(frames?.faceFrames);
  const effectSequence = buildSequence(frames?.effectFrames);

  // 繪製迴圈：貼圖就緒後以 rAF 驅動；暫停時凍結在 timeRef 記錄的當前幀。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frames || !images || !bodySequence) return;
    if (size.width === 0 || size.height === 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const box = computeBoundingBox(frames, images);
    if (!box) return;

    const margin = 8;
    const fit = Math.min(
      (size.width - margin * 2) / box.width,
      (size.height - margin * 2) / box.height,
    );
    // 放得下的話取整數倍（3x/4x…）；放不下才退成分數縮小。
    const scale = fit >= 1 ? Math.max(1, Math.floor(fit)) : fit;
    const originX = Math.round((size.width - box.width * scale) / 2 - box.minX * scale);
    const originY = Math.round((size.height - box.height * scale) / 2 - box.minY * scale);

    const renderAt = (time: number) => {
      composeFrame(
        ctx,
        frames,
        images,
        frameIndexAt(bodySequence, time),
        faceSequence ? frameIndexAt(faceSequence, time) : -1,
        effectSequence ? frameIndexAt(effectSequence, time) : -1,
        originX,
        originY,
        scale,
      );
    };

    if (!playing || !active) {
      // 暫停/切到背景：凍結在目前時間（不跳回第 0 幀）。
      renderAt(timeRef.current);
      return;
    }
    let raf = 0;
    // 從凍結時間繼續：start 補償已播放的時間，不從頭來。
    const start = performance.now() - timeRef.current;
    const tick = (now: number) => {
      timeRef.current = now - start;
      renderAt(timeRef.current);
      raf = window.requestAnimationFrame(tick);
    };
    raf = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(raf);
    // bodySequence/faceSequence/effectSequence 由 frames 推導，不需列入依賴。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frames, images, playing, active, size]);

  // 匯出彈窗的 Escape 關閉（同 ChangelogDialog 慣例）。
  useEffect(() => {
    if (!exportDialog) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !exporting) setExportDialog(false);
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [exportDialog, exporting]);

  const onDownload = () => {
    if (!frames || !images) return;
    if (!playing) {
      // 暫停中：直接下載目前畫面 PNG。
      const canvas = canvasRef.current;
      if (canvas) {
        exportCanvasPng(canvas, action).catch((nextError) =>
          setImagesError(String(nextError)),
        );
      }
      return;
    }
    // 播放中：先問格式（GIF / WebP）。
    setExportError(null);
    setExportDialog(true);
  };

  const runExport = async (kind: "gif" | "webp") => {
    if (!frames || !images) return;
    setExporting(kind);
    setExportError(null);
    try {
      const scene = { frames, images, action };
      if (kind === "gif") await exportGif(scene);
      else await exportWebP(scene);
      setExportDialog(false);
    } catch (nextError) {
      setExportError(String(nextError));
    } finally {
      setExporting(null);
    }
  };

  const waitingImages = !!frames && !images && !imagesError;
  const status = error
    ? { text: `角色合成失敗：${error}`, isError: true }
    : imagesError
      ? { text: `貼圖載入失敗：${imagesError}`, isError: true }
      : loading || waitingImages
        ? { text: "正在載入角色貼圖…", isError: false }
        : !frames
          ? { text: "尚無角色資料", isError: false }
          : null;

  return (
    <div className="avatar-preview" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        className="avatar-preview-canvas"
        width={Math.max(1, size.width)}
        height={Math.max(1, size.height)}
      />
      <button
        type="button"
        className="avatar-download-button"
        title={playing ? "下載動畫（GIF / WebP）" : "下載目前畫面 PNG"}
        aria-label="下載預覽圖"
        disabled={!frames || !images}
        onClick={onDownload}
      >
        {/* Lucide: arrow-down-to-line（https://lucide.dev/icons/arrow-down-to-line）*/}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M12 17V3" />
          <path d="m6 11 6 6 6-6" />
          <path d="M19 21H5" />
        </svg>
      </button>
      {status ? (
        <div className={`avatar-preview-status${status.isError ? " is-error" : ""}`} role="status">
          {status.text}
        </div>
      ) : null}
      {exportDialog ? (
        <div
          className="dialog-backdrop"
          onClick={() => {
            if (!exporting) setExportDialog(false);
          }}
        >
          <div
            className="avatar-export-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="下載動畫"
            onClick={(event) => event.stopPropagation()}
          >
            <header className="changelog-dialog-header">
              <h2>下載動畫</h2>
              <button
                className="icon-button"
                type="button"
                disabled={exporting !== null}
                onClick={() => setExportDialog(false)}
                aria-label="關閉"
              >
                ✕
              </button>
            </header>
            <div className="avatar-export-dialog-content">
              <p>依目前的動作與表情合成整個循環，選擇要匯出的格式：</p>
              <div className="avatar-export-actions">
                <button
                  type="button"
                  disabled={exporting !== null}
                  onClick={() => void runExport("gif")}
                >
                  {exporting === "gif" ? "匯出中…" : "GIF"}
                </button>
                <button
                  type="button"
                  disabled={exporting !== null}
                  onClick={() => void runExport("webp")}
                >
                  {exporting === "webp" ? "匯出中…" : "WebP"}
                </button>
              </div>
              {exportError ? (
                <p className="avatar-export-error" role="alert">
                  匯出失敗：{exportError}
                </p>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
});
