import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getAvatarEmotions,
  getAvatarEquipment,
  getAvatarFrames,
  getAvatarMeta,
  type AvatarFrames,
  type AvatarMeta,
  type EquipItem,
  type PartInput,
  type SkinInfo,
} from "../api";
import { AvatarEquipPanel } from "./AvatarEquipPanel";
import { AvatarPreview } from "./AvatarPreview";
import { AvatarShop } from "./AvatarShop";
import { joinColorId, resolveColorVariant, splitColorId } from "./avatarColor";

// 可穿的裝備部位（body/head 是膚色，不在此列）。
export type EquipSlot =
  | "cap"
  | "coat"
  | "longcoat"
  | "pants"
  | "shoes"
  | "glove"
  | "cape"
  | "shield"
  | "weapon"
  | "earrings"
  | "faceAcc"
  | "eyeAcc"
  | "hair"
  | "face";

export interface AvatarPartState {
  id: string;
  visible: boolean;
}

/** 已穿部位表；初始全空 = 裸身。 */
export type PartsState = Partial<Record<EquipSlot, AvatarPartState>>;

// 沒穿臉型時用的預設臉（決定表情清單與裸身表情貼圖）。
const DEFAULT_FACE_ID = "00020000";

// 紙娃娃頁：所有試穿狀態集中在這裡，預覽/控制區/商店都是受控元件。
export function AvatarPage({
  visible,
  cashOnly,
  onPickItem,
}: {
  visible: boolean;
  cashOnly: boolean;
  /** 點商店卡片時回報裝備 img 路徑（給 App footer 麵包屑）。 */
  onPickItem?: (path: string) => void;
}) {
  const [meta, setMeta] = useState<AvatarMeta | null>(null);
  const [equipment, setEquipment] = useState<EquipItem[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [skin, setSkin] = useState<SkinInfo | null>(null);
  const [action, setAction] = useState("stand1");
  const [emotion, setEmotion] = useState("default");
  const [emotions, setEmotions] = useState<string[]>(["default"]);
  const [playing, setPlaying] = useState(true);
  const [parts, setParts] = useState<PartsState>({});
  // 髮色/臉色（0–7）：穿上或換色時套用，見 avatarColor.ts。
  const [hairColor, setHairColor] = useState(0);
  const [faceColor, setFaceColor] = useState(0);
  // 帽子遮髮（對齊 WzComparerR2 的 HairCover）：勾選且有戴帽 → 帽子蓋住髮型；
  // 不勾 → 髮型超出帽子。沒戴帽時無效果。
  const [hairCover, setHairCover] = useState(true);
  const [frames, setFrames] = useState<AvatarFrames | null>(null);
  const [framesLoading, setFramesLoading] = useState(false);
  const [framesError, setFramesError] = useState<string | null>(null);
  const framesGenerationRef = useRef(0);

  // 換色時查 variant 是否存在用（全裝備 id 集合）。
  const equipmentIds = useMemo(
    () => new Set((equipment ?? []).map((item) => item.id)),
    [equipment],
  );
  const idExists = useCallback((id: string) => equipmentIds.has(id), [equipmentIds]);

  // 沒穿臉型時用預設臉，但跟著臉色選擇換色（該色不存在時退回顏色 0）。
  const faceId = parts.face?.id ?? resolveColorVariant("face", DEFAULT_FACE_ID, faceColor, idExists);

  // 目前臉型（穿著的或預設臉）實際可選的顏色：0–7 恆列出，
  // 8（白色）只有該造型真的有這個 id 才列出。
  const faceColorOptions = useMemo(() => {
    const split = splitColorId("face", faceId);
    if (!split) return [0, 1, 2, 3, 4, 5, 6, 7];
    const options = [0, 1, 2, 3, 4, 5, 6, 7];
    if (idExists(joinColorId("face", split.base, 8))) options.push(8);
    return options;
  }, [faceId, idExists]);

  // meta + equipment：第一次切到紙娃娃（此元件掛載）才載入，之後保持。
  useEffect(() => {
    let active = true;
    Promise.all([getAvatarMeta(), getAvatarEquipment()])
      .then(([nextMeta, nextEquipment]) => {
        if (!active) return;
        setMeta(nextMeta);
        setEquipment(nextEquipment);
        setSkin(nextMeta.skins[0] ?? null);
        // 後端動作清單若沒有 stand1，退用第一個動作。
        if (!nextMeta.actions.includes("stand1") && nextMeta.actions.length > 0) {
          setAction(nextMeta.actions[0]);
        }
      })
      .catch((nextError) => {
        if (active) setLoadError(String(nextError));
      });
    return () => {
      active = false;
    };
  }, []);

  // 表情清單跟著臉型走；換臉型時 emotion 重設 default。
  useEffect(() => {
    let active = true;
    getAvatarEmotions(faceId)
      .then((list) => {
        if (!active) return;
        setEmotions(list.length > 0 ? list : ["default"]);
        setEmotion("default");
      })
      .catch(() => {
        if (!active) return;
        setEmotions(["default"]);
        setEmotion("default");
      });
    return () => {
      active = false;
    };
  }, [faceId]);

  // 任何 parts/skin/action/emotion/hairCover 變更 → 重取合成幀（generation guard 防舊回應覆蓋）。
  // 只送 visible 的部位 + body/head：隱藏的部位（例如帽子）不能觸發後端遮髮，所以不送。
  useEffect(() => {
    if (!skin) return;
    const generation = ++framesGenerationRef.current;
    const partInputs: PartInput[] = [
      { slot: "body", id: skin.bodyId, visible: true },
      { slot: "head", id: skin.headId, visible: true },
    ];
    for (const [slot, part] of Object.entries(parts)) {
      if (part?.visible) partInputs.push({ slot, id: part.id, visible: true });
    }
    setFramesLoading(true);
    setFramesError(null);
    getAvatarFrames(partInputs, action, emotion, hairCover)
      .then((result) => {
        if (generation === framesGenerationRef.current) setFrames(result);
      })
      .catch((nextError) => {
        if (generation === framesGenerationRef.current) setFramesError(String(nextError));
      })
      .finally(() => {
        if (generation === framesGenerationRef.current) setFramesLoading(false);
      });
  }, [skin, action, emotion, parts, hairCover]);

  // 穿上（同 slot 取代），含 longcoat 互斥（移植自參考專案 WzComparerR2.Avatar）。
  const equip = useCallback((slot: EquipSlot, id: string) => {
    setParts((previous) => {
      const next: PartsState = { ...previous };
      if (slot === "longcoat") {
        // 穿套服時，若上衣或褲裙可見 → 套服自己先隱藏（保留裝備不脫）。
        const blocked = (previous.coat?.visible ?? false) || (previous.pants?.visible ?? false);
        next.longcoat = { id, visible: !blocked };
      } else if (slot === "coat" || slot === "pants") {
        // 穿上衣/褲裙時，若已穿套服 → 套服轉為隱藏。
        next[slot] = { id, visible: true };
        if (next.longcoat) next.longcoat = { ...next.longcoat, visible: false };
      } else {
        next[slot] = { id, visible: true };
      }
      return next;
    });
  }, []);

  const removePart = useCallback((slot: EquipSlot) => {
    setParts((previous) => {
      const next = { ...previous };
      delete next[slot];
      return next;
    });
  }, []);

  // 換髮色/臉色：記住選擇；已穿著的同 slot 部位立即換成同造型的新顏色
  // （該造型沒有此色時退回顏色 0）。
  const changeColor = useCallback(
    (slot: "hair" | "face", color: number) => {
      if (slot === "hair") setHairColor(color);
      else setFaceColor(color);
      setParts((previous) => {
        const part = previous[slot];
        if (!part) return previous;
        const nextId = resolveColorVariant(slot, part.id, color, idExists);
        if (nextId === part.id) return previous;
        return { ...previous, [slot]: { ...part, id: nextId } };
      });
    },
    [idExists],
  );

  const toggleVisible = useCallback((slot: EquipSlot) => {
    setParts((previous) => {
      const part = previous[slot];
      if (!part) return previous;
      return { ...previous, [slot]: { ...part, visible: !part.visible } };
    });
  }, []);

  let content;
  if (loadError) {
    content = <p className="avatar-page-status">紙娃娃資料載入失敗：{loadError}</p>;
  } else if (!meta || !equipment) {
    content = <p className="avatar-page-status">正在載入紙娃娃資料…</p>;
  } else if (!skin) {
    content = <p className="avatar-page-status">找不到可用的膚色資料</p>;
  } else {
    content = (
      <>
        <aside className="avatar-left">
          <div className="avatar-left-preview">
            <AvatarPreview
              frames={frames}
              loading={framesLoading}
              error={framesError}
              playing={playing}
              active={visible}
              action={action}
            />
          </div>
          <div className="avatar-left-controls">
            <AvatarEquipPanel
              skins={meta.skins}
              skin={skin}
              onSkinChange={setSkin}
              actions={meta.actions}
              action={action}
              onActionChange={setAction}
              emotions={emotions}
              emotion={emotion}
              onEmotionChange={setEmotion}
              hairColor={hairColor}
              faceColor={faceColor}
              faceColors={faceColorOptions}
              onColorChange={changeColor}
              hairCover={hairCover}
              onHairCoverChange={setHairCover}
              playing={playing}
              onTogglePlaying={() => setPlaying((previous) => !previous)}
              parts={parts}
              equipment={equipment}
              onToggleVisible={toggleVisible}
              onRemove={removePart}
            />
          </div>
        </aside>
        <section className="avatar-right">
          <AvatarShop
            equipment={equipment}
            parts={parts}
            cashOnly={cashOnly}
            hairColor={hairColor}
            faceColor={faceColor}
            onEquip={equip}
            onRemove={removePart}
            onPickItem={onPickItem}
          />
        </section>
      </>
    );
  }

  return (
    <div className="avatar-page" hidden={!visible}>
      {content}
    </div>
  );
}
