// lib.dom 缺的 File System Access API 型別補丁。
// FileSystemSyncAccessHandle / DedicatedWorkerGlobalScope 只存在於 lib.webworker,
// 但不能整包引入(會與 lib.dom 衝突),這裡只補用到的最小介面。
// 與 lib.dom 既有的 FileSystemFileHandle / FileSystemDirectoryHandle 介面合併擴充。

interface FileSystemSyncAccessHandle {
  read(buffer: BufferSource, options?: { at?: number }): number;
  write(buffer: BufferSource, options?: { at?: number }): number;
  truncate(newSize: number): void;
  getSize(): number;
  flush(): void;
  close(): void;
}

interface FileSystemFileHandle {
  createSyncAccessHandle(): Promise<FileSystemSyncAccessHandle>;
}

interface FileSystemDirectoryHandle {
  keys(): AsyncIterableIterator<string>;
  values(): AsyncIterableIterator<FileSystemHandle>;
  entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
}

interface DedicatedWorkerGlobalScope {
  postMessage(message: unknown): void;
  onmessage: ((this: DedicatedWorkerGlobalScope, ev: MessageEvent) => unknown) | null;
}
