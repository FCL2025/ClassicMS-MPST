import { useEffect, useMemo, useState } from "react";
import type { EquipItem, SkinInfo } from "../api";
import type { EquipSlot, PartsState } from "./AvatarPage";
import { loadAvatarIcon, SLOT_TABS } from "./AvatarShop";
import { COLOR_LABELS } from "./avatarColor";

function AvatarEquipRow({
  slot,
  part,
  item,
  onToggleVisible,
  onRemove,
}: {
  slot: EquipSlot;
  part: { id: string; visible: boolean };
  item: EquipItem | undefined;
  onToggleVisible: (slot: EquipSlot) => void;
  onRemove: (slot: EquipSlot) => void;
}) {
  const [icon, setIcon] = useState<string | null>(null);
  const iconPath = item?.iconPath;

  useEffect(() => {
    if (!iconPath) return;
    let active = true;
    loadAvatarIcon(iconPath).then((url) => {
      if (active && url) setIcon(url);
    });
    return () => {
      active = false;
    };
  }, [iconPath]);

  return (
    <div
      className={`avatar-equipped-row${part.visible ? "" : " is-hidden"}`}
      role="button"
      tabIndex={0}
      title={part.visible ? "點擊隱藏此部位" : "點擊顯示此部位"}
      onClick={() => onToggleVisible(slot)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onToggleVisible(slot);
        }
      }}
    >
      <span className="avatar-equipped-icon">{icon ? <img src={icon} alt="" /> : null}</span>
      <span className="avatar-equipped-name">{item?.name ?? `#${part.id}`}</span>
      <span className="avatar-equipped-eye" aria-hidden="true">
        👁
      </span>
      <button
        type="button"
        className="avatar-equipped-remove"
        title="脫下"
        aria-label={`脫下 ${item?.name ?? part.id}`}
        onClick={(event) => {
          event.stopPropagation();
          onRemove(slot);
        }}
      >
        ×
      </button>
    </div>
  );
}

interface AvatarEquipPanelProps {
  skins: SkinInfo[];
  skin: SkinInfo;
  onSkinChange: (skin: SkinInfo) => void;
  actions: string[];
  action: string;
  onActionChange: (action: string) => void;
  emotions: string[];
  emotion: string;
  onEmotionChange: (emotion: string) => void;
  hairColor: number;
  faceColor: number;
  onColorChange: (slot: "hair" | "face", color: number) => void;
  hairCover: boolean;
  onHairCoverChange: (hairCover: boolean) => void;
  playing: boolean;
  onTogglePlaying: () => void;
  parts: PartsState;
  equipment: EquipItem[];
  onToggleVisible: (slot: EquipSlot) => void;
  onRemove: (slot: EquipSlot) => void;
}

// 左下控制區：膚色/表情/動作選擇 + 播放暫停 + 目前裝備清單（點列切換顯示、× 脫下）。
export function AvatarEquipPanel({
  skins,
  skin,
  onSkinChange,
  actions,
  action,
  onActionChange,
  emotions,
  emotion,
  onEmotionChange,
  hairColor,
  faceColor,
  onColorChange,
  hairCover,
  onHairCoverChange,
  playing,
  onTogglePlaying,
  parts,
  equipment,
  onToggleVisible,
  onRemove,
}: AvatarEquipPanelProps) {
  const equipById = useMemo(() => new Map(equipment.map((item) => [item.id, item])), [equipment]);
  // 已穿部位（含 hair/face），依 SLOT_TABS 順序顯示。
  const wornSlots = SLOT_TABS.map(({ slot }) => slot).filter((slot) => parts[slot]);

  return (
    <div className="avatar-controls">
      <div className="avatar-control-row">
        <label htmlFor="avatar-skin-select">膚色</label>
        <select
          id="avatar-skin-select"
          value={skin.bodyId}
          onChange={(event) => {
            // 換膚色 → bodyId/headId 一起換（SkinInfo 已配對好）。
            const next = skins.find((candidate) => candidate.bodyId === event.target.value);
            if (next) onSkinChange(next);
          }}
        >
          {skins.map((candidate) => (
            <option key={candidate.bodyId} value={candidate.bodyId}>
              {candidate.name}
            </option>
          ))}
        </select>
      </div>

      <div className="avatar-control-row">
        <label htmlFor="avatar-emotion-select">表情</label>
        <select
          id="avatar-emotion-select"
          value={emotion}
          onChange={(event) => onEmotionChange(event.target.value)}
        >
          {emotions.map((candidate) => (
            <option key={candidate} value={candidate}>
              {candidate}
            </option>
          ))}
        </select>
      </div>

      <div className="avatar-control-row">
        <label htmlFor="avatar-action-select">動作</label>
        <div className="avatar-action-row">
          <select
            id="avatar-action-select"
            value={action}
            onChange={(event) => onActionChange(event.target.value)}
          >
            {actions.map((candidate) => (
              <option key={candidate} value={candidate}>
                {candidate}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="avatar-play-button"
            title={playing ? "暫停" : "播放"}
            aria-label={playing ? "暫停" : "播放"}
            onClick={onTogglePlaying}
          >
            {playing ? "⏸" : "▶"}
          </button>
        </div>
      </div>

      {/* 臉型顏色 / 髮型顏色：左右並排。 */}
      <div className="avatar-control-row">
        <div className="avatar-color-row">
          <div className="avatar-color-field">
            <label htmlFor="avatar-face-color-select">臉型顏色</label>
            <select
              id="avatar-face-color-select"
              value={faceColor}
              onChange={(event) => onColorChange("face", Number(event.target.value))}
            >
              {COLOR_LABELS.map((label, index) => (
                <option key={label} value={index}>
                  {label}
                </option>
              ))}
            </select>
          </div>
          <div className="avatar-color-field">
            <label htmlFor="avatar-hair-color-select">髮型顏色</label>
            <select
              id="avatar-hair-color-select"
              value={hairColor}
              onChange={(event) => onColorChange("hair", Number(event.target.value))}
            >
              {COLOR_LABELS.map((label, index) => (
                <option key={label} value={index}>
                  {label}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      {/* 帽子遮髮（hairCover）：勾選時帽子蓋住髮型；不勾時髮型超出帽子。 */}
      <div className="avatar-control-row">
        <label
          className="avatar-hair-cover"
          title="勾選時帽子蓋住髮型；不勾時髮型超出帽子（有戴帽子才生效）"
        >
          <input
            type="checkbox"
            checked={hairCover}
            onChange={(event) => onHairCoverChange(event.target.checked)}
          />
          帽子遮髮
        </label>
      </div>

      <hr className="avatar-divider" />

      <p className="avatar-equipped-title">目前裝備</p>
      {wornSlots.length === 0 ? (
        <p className="avatar-equipped-empty">尚未穿上任何裝備，從右邊挑一件吧。</p>
      ) : (
        wornSlots.map((slot) => {
          const part = parts[slot]!;
          return (
            <AvatarEquipRow
              key={slot}
              slot={slot}
              part={part}
              item={equipById.get(part.id)}
              onToggleVisible={onToggleVisible}
              onRemove={onRemove}
            />
          );
        })
      )}
    </div>
  );
}
