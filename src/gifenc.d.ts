// gifenc（純 JS GIF 編碼器）沒有附型別，這裡只補本專案用到的 API。
declare module "gifenc" {
  export interface QuantizeOptions {
    format?: "rgb565" | "rgb444" | "rgba4444";
    clearAlpha?: boolean;
    clearAlphaColor?: number;
    clearAlphaThreshold?: number;
    oneBitAlpha?: boolean | number;
    useSqrt?: boolean;
  }

  /** 回傳調色盤；format 為 rgba4444 時每筆是 [r,g,b,a]，否則 [r,g,b]。 */
  export function quantize(
    rgba: Uint8Array | Uint8ClampedArray,
    maxColors: number,
    options?: QuantizeOptions,
  ): number[][];

  export function applyPalette(
    rgba: Uint8Array | Uint8ClampedArray,
    palette: number[][],
    format?: "rgb565" | "rgb444" | "rgba4444",
  ): Uint8Array;

  export interface WriteFrameOptions {
    palette?: number[][];
    delay?: number;
    /** -1 = 播放一次,0 = 無限循環,>0 = 次數。 */
    repeat?: number;
    transparent?: boolean;
    transparentIndex?: number;
    dispose?: number;
  }

  export interface GifEncoderStream {
    writeFrame(
      index: Uint8Array,
      width: number,
      height: number,
      options?: WriteFrameOptions,
    ): void;
    finish(): void;
    /** 完整 GIF 檔案內容（獨立 buffer 的 Uint8Array）。 */
    bytes(): Uint8Array;
  }

  export function GIFEncoder(): GifEncoderStream;
}
