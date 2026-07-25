import {
  useCallback,
  useRef,
  useState,
  type PointerEvent,
  type WheelEvent,
} from "react";

interface ImagePreviewProps {
  alt: string;
  errorMessage?: string | null;
  imageUrl: string | null;
  loading: boolean;
  mode?: "exact" | "atlas";
  note?: string | null;
}

interface Point {
  x: number;
  y: number;
}

interface ImageSize {
  width: number;
  height: number;
  /** 這筆尺寸屬於哪個圖片 URL；用來避免 onLoad 與重置的競態把尺寸洗掉。 */
  url: string;
}

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 32;

function clampZoom(value: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, value));
}

export function ImagePreview({
  alt,
  errorMessage,
  imageUrl,
  loading,
  mode = "exact",
  note,
}: ImagePreviewProps) {
  const stageRef = useRef<HTMLDivElement>(null);
  const dragOriginRef = useRef<Point | null>(null);
  const [loadedSize, setLoadedSize] = useState<ImageSize | null>(null);
  const [zoom, setZoom] = useState(1);
  const [offset, setOffset] = useState<Point>({ x: 0, y: 0 });
  const [panning, setPanning] = useState(false);

  // 只採用屬於目前 imageUrl 的尺寸；data: URL 的 img onLoad 可能早於
  // 被動 effect 觸發，若用 effect 重置會把剛量到的尺寸清掉，導致預覽
  // 只剩 1px × zoom（小圖示永遠放大不了）。
  const imageSize =
    loadedSize && imageUrl && loadedSize.url === imageUrl ? loadedSize : null;

  const fitImage = useCallback(
    (size = imageSize) => {
      const stage = stageRef.current;
      if (!stage || !size) return;
      const availableWidth = Math.max(1, stage.clientWidth - 64);
      const availableHeight = Math.max(1, stage.clientHeight - 64);
      // 與 Harepacker-MPST 的 CanvasPreview.fitToWindow 一致：一律縮放到填滿
      // 視窗的 92%，包含把小型像素素材放大超過 100%——先前這裡把放大幅度
      // 硬性夾在 100% 以內，導致小圖示在預覽區裡小到看不清楚。
      setZoom(
        clampZoom(
          Math.min(
            (availableWidth * 0.92) / size.width,
            (availableHeight * 0.92) / size.height,
          ),
        ),
      );
      setOffset({ x: 0, y: 0 });
    },
    [imageSize],
  );

  const adjustZoom = (nextZoom: number) => {
    setZoom(clampZoom(nextZoom));
  };

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (!imageUrl) return;
    event.preventDefault();
    adjustZoom(zoom * (event.deltaY < 0 ? 1.12 : 0.89));
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (!imageUrl || event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    dragOriginRef.current = {
      x: event.clientX - offset.x,
      y: event.clientY - offset.y,
    };
    setPanning(true);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const origin = dragOriginRef.current;
    if (!origin) return;
    setOffset({
      x: event.clientX - origin.x,
      y: event.clientY - origin.y,
    });
  };

  const stopPanning = (event: PointerEvent<HTMLDivElement>) => {
    dragOriginRef.current = null;
    setPanning(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <section className="image-editor" aria-label="圖片預覽">
      <div className="image-editor-toolbar">
        <div className="image-editor-title">
          <strong>圖片預覽</strong>
          {imageSize ? (
            <span>
              原始 {imageSize.width} × {imageSize.height} px
            </span>
          ) : null}
          {mode === "atlas" ? (
            <span className="atlas-preview-badge" title={note ?? undefined}>
              完整 Atlas
            </span>
          ) : null}
        </div>
        <div className="image-editor-actions">
          <button
            type="button"
            aria-label="縮小圖片"
            disabled={!imageUrl}
            onClick={() => adjustZoom(zoom / 1.2)}
          >
            −
          </button>
          <button
            className="zoom-readout"
            type="button"
            disabled={!imageUrl}
            title="相對於原始像素尺寸；按一下重設為 100%"
            onClick={() => {
              setZoom(1);
              setOffset({ x: 0, y: 0 });
            }}
          >
            {Math.round(zoom * 100)}%
          </button>
          <button
            type="button"
            aria-label="放大圖片"
            disabled={!imageUrl}
            onClick={() => adjustZoom(zoom * 1.2)}
          >
            +
          </button>
          <button
            className="fit-button"
            type="button"
            disabled={!imageUrl}
            title="縮放至填滿預覽區域（大圖縮小、小圖放大）"
            onClick={() => fitImage()}
          >
            適合視窗
          </button>
        </div>
      </div>
      <div
        ref={stageRef}
        className={`image-editor-stage${panning ? " is-panning" : ""}${
          imageUrl ? " is-pan-ready" : ""
        }`}
        onDoubleClick={() => fitImage()}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={stopPanning}
        onPointerCancel={stopPanning}
        onWheel={onWheel}
      >
        {imageUrl ? (
          <div
            className="image-editor-frame"
            style={{
              width: Math.max(1, Math.round((imageSize?.width ?? 1) * zoom)),
              height: Math.max(1, Math.round((imageSize?.height ?? 1) * zoom)),
              left: `calc(50% + ${offset.x}px)`,
              top: `calc(50% + ${offset.y}px)`,
            }}
          >
            <img
              src={imageUrl}
              alt={alt}
              draggable={false}
              onLoad={(event) => {
                if (!imageUrl) return;
                const nextSize = {
                  url: imageUrl,
                  width: event.currentTarget.naturalWidth,
                  height: event.currentTarget.naturalHeight,
                };
                setLoadedSize(nextSize);
                window.requestAnimationFrame(() => fitImage(nextSize));
              }}
            />
          </div>
        ) : (
          <div className="image-editor-message">
            {loading
              ? "正在載入圖片…"
              : errorMessage
                ? `圖片解析失敗：${errorMessage}`
                : "這個節點沒有可預覽的圖片"}
          </div>
        )}
      </div>
      <div
        className={`image-editor-footer${mode === "atlas" ? " is-atlas" : ""}`}
        title={mode === "atlas" ? (note ?? undefined) : undefined}
      >
        {mode === "atlas"
          ? "無法定位唯一 frame，目前顯示完整 Atlas 頁"
          : "滾輪縮放 · 拖曳平移 · 雙擊適合視窗"}
      </div>
    </section>
  );
}
