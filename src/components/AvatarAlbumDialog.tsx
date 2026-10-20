import { useEffect, useRef, useState } from "react";
import type { AvatarLook } from "./avatarAlbum";

interface AvatarAlbumDialogProps {
  mode: "save" | "gallery";
  looks: AvatarLook[];
  thumbnail: string | null;
  defaultName: string;
  busy: boolean;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (name: string) => void;
  onApply: (look: AvatarLook) => void;
  onRename: (look: AvatarLook, name: string) => Promise<boolean>;
  onDelete: (look: AvatarLook) => Promise<boolean>;
  onStartSave: () => void;
}

export function AvatarAlbumDialog({
  mode,
  looks,
  thumbnail,
  defaultName,
  busy,
  loading,
  error,
  onClose,
  onSave,
  onApply,
  onRename,
  onDelete,
  onStartSave,
}: AvatarAlbumDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(defaultName);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [deletingId, setDeletingId] = useState<string | null>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    dialog.showModal();
    return () => dialog.close();
  }, []);

  useEffect(() => {
    if (mode === "save") {
      setName(defaultName);
      nameRef.current?.focus();
      nameRef.current?.select();
    }
  }, [mode, defaultName]);

  const startRename = (look: AvatarLook) => {
    setDeletingId(null);
    setEditingId(look.id);
    setEditingName(look.name);
  };

  return (
    <dialog
      ref={dialogRef}
      className="avatar-album-dialog"
      aria-labelledby="avatar-album-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
    >
      <header className="changelog-dialog-header">
        <h2 id="avatar-album-title">{mode === "save" ? "儲存目前外觀" : `我的相簿 · ${looks.length}`}</h2>
        <button type="button" className="icon-button" aria-label="關閉相簿" disabled={busy} onClick={onClose}>
          ✕
        </button>
      </header>

      {mode === "save" ? (
        <form
          className="avatar-album-save"
          onSubmit={(event) => {
            event.preventDefault();
            if (name.trim() && !busy) onSave(name.trim());
          }}
        >
          <div className="avatar-album-save-preview">
            {thumbnail ? <img src={thumbnail} alt="目前外觀預覽" /> : null}
          </div>
          <div className="avatar-album-save-fields">
            <label htmlFor="avatar-album-name">外觀名稱</label>
            <input
              ref={nameRef}
              id="avatar-album-name"
              value={name}
              maxLength={40}
              onChange={(event) => setName(event.target.value)}
              placeholder="例如：藍色套裝"
            />
            <p>會保存目前的裝備、膚色、顏色、動作與表情。</p>
            <div className="avatar-album-save-actions">
              <button type="button" disabled={busy} onClick={onClose}>取消</button>
              <button type="submit" className="is-primary" disabled={busy || !name.trim()}>
                {busy ? "儲存中…" : "儲存外觀"}
              </button>
            </div>
          </div>
        </form>
      ) : (
        <>
          <div className="avatar-album-toolbar">
            <p>點選外觀即可套用到紙娃娃。</p>
            <button type="button" disabled={busy || loading} onClick={onStartSave}>＋ 儲存目前外觀</button>
          </div>
          <div className="avatar-album-body">
            {loading ? (
              <div className="avatar-album-empty" role="status">正在讀取相簿…</div>
            ) : looks.length === 0 ? (
              <div className="avatar-album-empty">
                <span aria-hidden="true">▧</span>
                <strong>相簿還沒有外觀</strong>
                <p>搭配好紙娃娃後，儲存一套外觀就能隨時回來選用。</p>
              </div>
            ) : (
              <div className="avatar-album-grid">
                {looks.map((look) => (
                  <div className="avatar-album-card" key={look.id}>
                    <button type="button" className="avatar-album-card-select" disabled={busy} onClick={() => onApply(look)}>
                      <span className="avatar-album-card-image"><img src={look.thumbnail} alt="" /></span>
                      <span className="avatar-album-card-name">{look.name}</span>
                      <span className="avatar-album-card-hint">點選套用</span>
                    </button>
                    {editingId === look.id ? (
                      <form
                        className="avatar-album-card-edit"
                        onSubmit={(event) => {
                          event.preventDefault();
                          if (editingName.trim() && !busy) {
                            void onRename(look, editingName.trim()).then((saved) => {
                              if (saved) setEditingId(null);
                            });
                          }
                        }}
                      >
                        <input
                          aria-label="外觀名稱"
                          value={editingName}
                          maxLength={40}
                          autoFocus
                          onChange={(event) => setEditingName(event.target.value)}
                        />
                        <button type="submit" disabled={busy || !editingName.trim()}>確定</button>
                        <button type="button" onClick={() => setEditingId(null)}>取消</button>
                      </form>
                    ) : deletingId === look.id ? (
                      <div className="avatar-album-card-confirm">
                        <span>確定刪除？</span>
                        <button type="button" disabled={busy} onClick={() => {
                          void onDelete(look).then((deleted) => {
                            if (deleted) setDeletingId(null);
                          });
                        }}>刪除</button>
                        <button type="button" disabled={busy} onClick={() => setDeletingId(null)}>取消</button>
                      </div>
                    ) : (
                      <div className="avatar-album-card-actions">
                        <button type="button" disabled={busy} onClick={() => startRename(look)}>重新命名</button>
                        <button type="button" disabled={busy} onClick={() => { setEditingId(null); setDeletingId(look.id); }}>刪除</button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      )}
      {error ? <p className="avatar-album-error" role="alert">{error}</p> : null}
      <p className="avatar-album-note">相簿只存在此瀏覽器；清除網站資料會一併刪除，其他裝置不會同步。</p>
    </dialog>
  );
}
