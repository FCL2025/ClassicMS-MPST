// 技能瀏覽器的中繼資料：職業對照表、職業群分組、每級數值 prop 的中文標籤、
// 以及楓之谷字串（\n 換行、#c…# 強調、#prop 佔位符）的解析工具。
// 遊戲字串表（String/TW/Skill）不含職業名稱，職業顯示名由此表提供。

export type JobGroup = "冒險家" | "皇家騎士團" | "英雄" | "其他";

export interface JobMeta {
  name: string;
  /** 轉職階段（一轉/二轉…）；新手等不分階段的為 null。 */
  tier: string | null;
  group: JobGroup;
}

/** (id, 名稱, 轉職階段) 批次登記用。 */
type JobEntry = [id: string, name: string, tier?: string];

function buildJobMeta(): Record<string, JobMeta> {
  const table: Record<string, JobMeta> = {};
  const register = (group: JobGroup, entries: JobEntry[]) => {
    for (const [id, name, tier] of entries) {
      table[id] = { name, tier: tier ?? null, group };
    }
  };
  register("冒險家", [
    ["000", "新手"],
    ["100", "劍士", "一轉"],
    ["110", "狂戰士", "二轉"],
    ["111", "十字軍", "三轉"],
    ["112", "英雄", "四轉"],
    ["120", "準騎士", "二轉"],
    ["121", "騎士", "三轉"],
    ["122", "聖騎士", "四轉"],
    ["130", "槍戰士", "二轉"],
    ["131", "龍騎士", "三轉"],
    ["132", "黑騎士", "四轉"],
    ["200", "魔法師", "一轉"],
    ["210", "巫師（火、毒）", "二轉"],
    ["211", "魔導士（火、毒）", "三轉"],
    ["212", "大魔導士（火、毒）", "四轉"],
    ["220", "巫師（冰、雷）", "二轉"],
    ["221", "魔導士（冰、雷）", "三轉"],
    ["222", "大魔導士（冰、雷）", "四轉"],
    ["230", "僧侶", "二轉"],
    ["231", "祭司", "三轉"],
    ["232", "主教", "四轉"],
    ["300", "弓箭手", "一轉"],
    ["310", "獵人", "二轉"],
    ["311", "遊俠", "三轉"],
    ["312", "箭神", "四轉"],
    ["320", "弩弓手", "二轉"],
    ["321", "狙擊手", "三轉"],
    ["322", "神射手", "四轉"],
    ["400", "盜賊", "一轉"],
    ["410", "刺客", "二轉"],
    ["411", "暗殺者", "三轉"],
    ["412", "夜使者", "四轉"],
    ["420", "俠盜", "二轉"],
    ["421", "神偷", "三轉"],
    ["422", "暗影神偷", "四轉"],
    ["500", "海盜", "一轉"],
    ["510", "拳手", "二轉"],
    ["511", "格鬥家", "三轉"],
    ["512", "拳霸", "四轉"],
    ["520", "槍手", "二轉"],
    ["521", "神槍手", "三轉"],
    ["522", "槍神", "四轉"],
  ]);
  register("皇家騎士團", [
    ["1000", "貴族"],
    ["1100", "聖魂劍士", "一轉"],
    ["1110", "聖魂劍士", "二轉"],
    ["1111", "聖魂劍士", "三轉"],
    ["1112", "聖魂劍士", "四轉"],
    ["1200", "烈焰巫師", "一轉"],
    ["1210", "烈焰巫師", "二轉"],
    ["1211", "烈焰巫師", "三轉"],
    ["1212", "烈焰巫師", "四轉"],
    ["1300", "破風使者", "一轉"],
    ["1310", "破風使者", "二轉"],
    ["1311", "破風使者", "三轉"],
    ["1312", "破風使者", "四轉"],
    ["1400", "暗夜行者", "一轉"],
    ["1410", "暗夜行者", "二轉"],
    ["1411", "暗夜行者", "三轉"],
    ["1412", "暗夜行者", "四轉"],
    ["1500", "閃雷悍將", "一轉"],
    ["1510", "閃雷悍將", "二轉"],
    ["1511", "閃雷悍將", "三轉"],
    ["1512", "閃雷悍將", "四轉"],
  ]);
  register("英雄", [
    ["2000", "戰童"],
    ["2100", "狂狼勇士", "一轉"],
    ["2110", "狂狼勇士", "二轉"],
    ["2111", "狂狼勇士", "三轉"],
    ["2112", "狂狼勇士", "四轉"],
    ["2200", "龍魔導士"],
    ["2210", "龍魔導士", "一轉"],
    ["2211", "龍魔導士", "二轉"],
    ["2212", "龍魔導士", "三轉"],
    ["2213", "龍魔導士", "四轉"],
    ["2214", "龍魔導士", "五轉"],
    ["2215", "龍魔導士", "六轉"],
    ["2216", "龍魔導士", "七轉"],
    ["2217", "龍魔導士", "八轉"],
    ["2218", "龍魔導士", "九轉"],
  ]);
  register("其他", [
    ["800", "騎寵"],
    ["900", "管理員"],
  ]);
  return table;
}

const JOB_META = buildJobMeta();

/** 查職業顯示資訊；對照表沒有的 id 歸到「其他」。 */
export function jobMeta(job: string): JobMeta {
  return JOB_META[job] ?? { name: `職業 ${job}`, tier: null, group: "其他" };
}

/**
 * 左欄的分區標題：冒險家細分成 新手/劍士/法師/弓箭手/盜賊/海盜（依職業 id 首位），
 * 皇家騎士團與英雄維持單一區塊；對照表沒有的 id 歸「其他」。
 */
export function jobSection(job: string): string {
  const meta = jobMeta(job);
  if (meta.group !== "冒險家") return meta.group;
  if (job === "000") return "新手・冒險家";
  const family = JOB_FAMILY[job.charAt(0)];
  return family ? `${family}・冒險家` : "其他";
}

/** 冒險家職業 id 首位 → 系譜名（一轉職業名）。 */
const JOB_FAMILY: Record<string, string> = {
  "1": "劍士",
  "2": "法師",
  "3": "弓箭手",
  "4": "盜賊",
  "5": "海盜",
};

/** 左欄分區的顯示順序。 */
export const SECTION_ORDER: string[] = [
  "新手・冒險家",
  "劍士・冒險家",
  "法師・冒險家",
  "弓箭手・冒險家",
  "盜賊・冒險家",
  "海盜・冒險家",
  "皇家騎士團",
  "英雄",
  "其他",
];

/** 每級數值 prop 的中文標籤與單位；查不到的 key 原樣顯示。 */
const PROP_META: Record<string, { label: string; unit?: "%" | "秒" }> = {
  mpCon: { label: "MP 消耗" },
  hpCon: { label: "HP 消耗" },
  damage: { label: "傷害", unit: "%" },
  prop: { label: "機率", unit: "%" },
  attackCount: { label: "攻擊次數" },
  mobCount: { label: "攻擊怪物數" },
  targetPlus: { label: "額外攻擊目標" },
  time: { label: "持續時間", unit: "秒" },
  cooldown: { label: "冷卻時間", unit: "秒" },
  interval: { label: "觸發間隔", unit: "秒" },
  range: { label: "射程範圍" },
  pad: { label: "物理攻擊力" },
  pdd: { label: "物理防禦力" },
  mad: { label: "魔法攻擊力" },
  mdd: { label: "魔法防禦力" },
  acc: { label: "命中值" },
  eva: { label: "迴避值" },
  speed: { label: "移動速度" },
  jump: { label: "跳躍力" },
  mastery: { label: "熟練度", unit: "%" },
  ter: { label: "屬性抗性", unit: "%" },
  asr: { label: "狀態異常抗性", unit: "%" },
  cr: { label: "爆擊機率", unit: "%" },
  bulletCount: { label: "子彈數量" },
  bulletCon: { label: "子彈消耗" },
  moneyCon: { label: "楓幣消耗" },
  itemCon: { label: "道具消耗" },
  morph: { label: "變身" },
  hp: { label: "HP" },
  mp: { label: "MP" },
  x: { label: "數值 X" },
  y: { label: "數值 Y" },
  z: { label: "數值 Z" },
  expPerReduce: { label: "死亡時經驗值減少", unit: "%" },
  lt: { label: "範圍左上" },
  rb: { label: "範圍右下" },
};

export type PropValue = number | string | [number, number];

export function propLabel(key: string): string {
  return PROP_META[key]?.label ?? key;
}

export function formatPropValue(key: string, value: PropValue): string {
  if (Array.isArray(value)) return `(${value[0]}, ${value[1]})`;
  if (typeof value === "number") {
    const unit = PROP_META[key]?.unit;
    return unit ? `${value}${unit}` : value.toLocaleString();
  }
  return value;
}

/** 不顯示在數值表的 prop（hs 是內部字串參照，對使用者無意義）。 */
export const HIDDEN_PROPS = new Set(["hs"]);

export interface TextSegment {
  text: string;
  accent: boolean;
  /** 道具 icon 段（`#i4031003#`）：text 為空，由呼叫端渲染 icon。 */
  itemId?: number;
  /** 道具名稱段（`#t2022000#`）：text 為空，由呼叫端渲染名稱。 */
  itemNameId?: number;
}

export interface MapleLine {
  segments: TextSegment[];
  /** 玩家台詞（`#L0#` 等標記，可跨 `#b` 等樣式碼、以 `#l` 收尾）；缺省為 NPC 台詞。 */
  player?: boolean;
}

/**
 * 解析楓之谷字串：換行是字面 `\n` / `\r` 兩字元（偶爾夾帶真正的 CR 字元）。
 * 標記處理：`#c…#` 自足強調（純數字 `#c4000015#` 是遊戲的「目前持有數」
 * 小工具，靜態檢視無意義，直接去掉）、`#b…#k` / `#e…#n` / `#r…#k` 區間強調、
 * `#L<n>#…#l` 玩家台詞（去掉標記、整行標 player）、
 * `#i<id>#` 道具 icon 段、`#t<id>#` 道具名稱段（由呼叫端渲染）、
 * `#p2101#` / `#o…#` / `#m…#` 引用標記不查表直接去掉（後端已代換名字）；
 * 其他 #token（如 #prop）原樣保留。
 */
export function parseMapleText(raw: string): MapleLine[] {
  // 換行標記是字面 `\n` / `\r` 兩字元（偶爾夾帶真正的 CR 字元，一併視為換行）；
  // 轉義的 \# 先換成 NUL 佔位，避免被當成標記起點，最後再還原。
  const normalized = raw.replace(/\\[nr]|\r/g, "\n").replace(/\\#/g, "\0");
  return normalized.split("\n").map((line) => {
    // 玩家台詞標記 #L<n>#：可能在行首，也可能前面夾 #b 之類的樣式碼。
    const playerMark = /^(?:#[bkre])*#L\d+#/.exec(line);
    const segments = parseMapleLine(playerMark ? line.slice(playerMark[0].length) : line);
    return playerMark ? { segments, player: true } : { segments };
  });
}

function parseMapleLine(line: string): TextSegment[] {
  const segments: TextSegment[] = [];
  let accent = false;
  let buffer = "";
  const flush = () => {
    if (buffer) {
      segments.push({ text: buffer.replace(/\0/g, "#"), accent });
      buffer = "";
    }
  };
  let i = 0;
  while (i < line.length) {
    // 區間強調標記（不自閉合，文字直接接在後面）：#b…#k、#e…#n、#r…#k。
    if (line.startsWith("#b", i) || line.startsWith("#e", i) || line.startsWith("#r", i)) {
      flush();
      accent = true;
      i += 2;
      continue;
    }
    if (line.startsWith("#k", i) || line.startsWith("#n", i)) {
      flush();
      accent = false;
      i += 2;
      continue;
    }
    // #l 是 #L<n># 玩家台詞的收尾標記，無顯示意義。
    if (line.startsWith("#l", i)) {
      i += 2;
      continue;
    }
    // 道具 icon（#i<id>#）與道具名稱（#t<id>#）：結尾偶爾是冒號，轉對應段。
    const item = /^#([it])(\d+)[:#]/.exec(line.slice(i));
    if (item) {
      flush();
      if (item[1] === "i") segments.push({ text: "", accent, itemId: Number(item[2]) });
      else segments.push({ text: "", accent, itemNameId: Number(item[2]) });
      i += item[0].length;
      continue;
    }
    // 引用標記（NPC/怪物/地圖）：後端已代換名字；殘留的不查表直接去掉。
    const ref = /^#[pom]\d+#/.exec(line.slice(i));
    if (ref) {
      i += ref[0].length;
      continue;
    }
    if (line[i] === "#") {
      const end = line.indexOf("#", i + 1);
      if (end > i + 1) {
        const token = line.slice(i + 1, end);
        // 自足強調標記 #c…#；純數字（#c4000015#）是遊戲的持有數小工具，去掉。
        if (token.startsWith("c")) {
          flush();
          if (token.length > 1 && !/^c\d+$/.test(token)) {
            segments.push({ text: token.slice(1), accent: true });
          }
          i = end + 1;
          continue;
        }
      }
      // 未知或未閉合標記：# 視為一般字元。
      buffer += "#";
      i += 1;
      continue;
    }
    buffer += line[i];
    i += 1;
  }
  flush();
  return segments;
}

/**
 * 把技能 H 模板（如「出現概率#prop%，發揮#damage%的最終攻擊」）的
 * #key 佔位符換成指定等級的數值；查不到的 key 原樣保留。
 */
export function substituteProps(
  template: string,
  props: Record<string, PropValue>,
): string {
  return template.replace(/#([A-Za-z][A-Za-z0-9]*)/g, (match, key: string) => {
    if (key === "c") return match; // 強調標記，交給 parseMapleText 處理
    const value = props[key];
    if (typeof value === "number") return String(value);
    if (typeof value === "string") return value;
    return match;
  });
}
