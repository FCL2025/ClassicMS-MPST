import { useEffect, useRef, useState } from "react";
import { exportFile, openExternal, pickExportPath, type ExportFormat } from "../api";

const DISCORD_INVITE_URL = "https://discord.gg/BnnAympQZW";

interface ExportMenuProps {
  canExportPng: boolean;
  nodeName: string | null;
  path: string | null;
  onMessage: (message: string, tone?: "success" | "error") => void;
  onShowChangelog: () => void;
}

interface ExportOption {
  description: string;
  extension: ExportFormat;
  label: string;
}

const EXPORT_OPTIONS: ExportOption[] = [
  { extension: "json", label: "JSON 資料", description: "匯出目前節點與完整子樹" },
  { extension: "png", label: "PNG 圖片", description: "匯出目前節點的解析圖片" },
  { extension: "xml", label: "XML 資料", description: "匯出完整子樹，不包含圖片" },
];

function safeFileName(value: string): string {
  const sanitized = value.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").trim();
  return sanitized || "export";
}

export function ExportMenu({ canExportPng, nodeName, path, onMessage, onShowChangelog }: ExportMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [exporting, setExporting] = useState<ExportFormat | null>(null);

  useEffect(() => {
    const closeOnPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnPointerDown);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnPointerDown);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, []);

  const runExport = async (option: ExportOption) => {
    if (!path || exporting) return;
    setOpen(false);
    try {
      const outputPath = await pickExportPath({
        title: `匯出 ${option.label}`,
        defaultName: `${safeFileName(nodeName ?? path.split("/").at(-1) ?? "export")}.${option.extension}`,
        label: option.label,
        extension: option.extension,
      });
      if (!outputPath) return;
      setExporting(option.extension);
      const result = await exportFile(path, option.extension, outputPath);
      onMessage(
        `已匯出 ${option.label}（${result.nodes.toLocaleString()} 個節點）→ ${result.out_path}`,
        "success",
      );
    } catch (error) {
      onMessage(`匯出失敗：${String(error)}`, "error");
    } finally {
      setExporting(null);
    }
  };

  return (
    <nav className="app-menu-bar" aria-label="應用程式選單">
      <div className={`app-menu-group${open ? " is-open" : ""}`} ref={menuRef}>
        <button
          className="app-menu-trigger"
          type="button"
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={() => setOpen((current) => !current)}
        >
          匯出
        </button>
        {open ? (
          <div className="app-menu-dropdown" role="menu">
            {EXPORT_OPTIONS.map((option) => (
              <button
                className="app-menu-item"
                type="button"
                role="menuitem"
                key={option.extension}
                disabled={
                  !path || exporting !== null || (option.extension === "png" && !canExportPng)
                }
                onClick={() => void runExport(option)}
              >
                <span className={`export-format-icon is-${option.extension}`} aria-hidden="true">
                  {option.extension.toUpperCase()}
                </span>
                <span>
                  <strong>
                    {exporting === option.extension ? "匯出中…" : `匯出為 ${option.label}`}
                  </strong>
                  <small>{option.description}</small>
                </span>
              </button>
            ))}
            {!path ? <p className="app-menu-hint">請先選擇一個資料節點</p> : null}
          </div>
        ) : null}
      </div>
      <button
        className="app-menu-trigger"
        type="button"
        onClick={() => void openExternal(DISCORD_INVITE_URL)}
      >
        加入 Discord
      </button>
      <button className="app-menu-trigger" type="button" onClick={onShowChangelog}>
        更新日誌
      </button>
    </nav>
  );
}
