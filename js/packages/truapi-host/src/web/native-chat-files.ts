import { bytesToHex } from "@parity/truapi/scale";
import type {
  NativeChatFileExportRequest,
  NativeChatFilePickRequest,
  NativeChatFilesHost,
  NativeChatPickedFile,
} from "../generated/host-callbacks.js";
import {
  inspectNativeChatFileMetadata,
  nativeChatExportFilename,
} from "./native-chat-media.js";

const MAX_FILE_SIZE = 0xffff_ffff;
const MAX_READ_SIZE = 2_000_000;
const DATABASE_NAME = "truapi-native-chat-files";
const SOURCE_STORE = "sources";

type FileContext = NativeChatFilePickRequest | NativeChatFileExportRequest;
type SavePicker = (options: {
  suggestedName: string;
}) => Promise<FileSystemFileHandle>;
type SourceRecord = { blob: Blob };
type ExportRecord = {
  writer: FileSystemWritableFileStream;
  size: number;
  written: number;
  queue: Promise<void>;
  removePartial: () => Promise<void>;
  presentCompleted?: () => Promise<void>;
};

/** Immutable host-private sources; writes resolve only after durable commit. */
export interface BrowserNativeChatFileSourceStore {
  putSources(
    sources: readonly { sourceId: string; blob: Blob }[],
  ): Promise<void>;
  readSource(sourceId: string): Promise<Blob | undefined>;
  releaseSource(sourceId: string): Promise<void>;
}

export interface BrowserNativeChatFilesHost extends NativeChatFilesHost {
  /** Close host UI and cancel partial exports, never release durable sources. */
  dispose(): void;
}

function checkedSize(size: number): void {
  if (!Number.isInteger(size) || size < 0 || size > MAX_FILE_SIZE) {
    throw new Error("Chat file size exceeds the supported range");
  }
}

function safeLabel(value: string): string {
  return value
    .replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/gu, " ")
    .slice(0, 200);
}

/** Trusted main-window custody. No filename, path or source handle crosses into a Guest. */
export function createBrowserNativeChatFilesHost(
  sourceStore?: BrowserNativeChatFileSourceStore,
): BrowserNativeChatFilesHost {
  let disposed = false;
  let database: Promise<IDBDatabase> | undefined;
  const dialogs = new Set<() => void>();
  const exports = new Map<string, ExportRecord>();
  const mediaAbort = new AbortController();

  function available(): void {
    if (disposed) throw new Error("Native Chat files are unavailable");
  }

  function openDatabase(cleanup = false): Promise<IDBDatabase> {
    if (!cleanup) available();
    if (!database) {
      database = new Promise<IDBDatabase>((resolve, reject) => {
        if (!globalThis.indexedDB) {
          reject(new Error("Durable Chat file storage is unavailable"));
          return;
        }
        const request = indexedDB.open(DATABASE_NAME, 1);
        let blocked = false;
        request.onupgradeneeded = () =>
          request.result.createObjectStore(SOURCE_STORE);
        request.onerror = () =>
          reject(new Error("Durable Chat file storage is unavailable"));
        request.onblocked = () => {
          blocked = true;
          reject(new Error("Durable Chat file storage is blocked"));
        };
        request.onsuccess = () => {
          const db = request.result;
          if (blocked || (disposed && !cleanup)) {
            db.close();
            reject(new Error("Native Chat files are unavailable"));
            return;
          }
          db.onversionchange = () => {
            db.close();
            database = undefined;
          };
          resolve(db);
        };
      }).catch((error: unknown) => {
        database = undefined;
        throw error;
      });
    }
    return database;
  }

  async function store<T>(
    mode: IDBTransactionMode,
    action: (store: IDBObjectStore) => IDBRequest<T>,
    cleanup = false,
  ): Promise<T> {
    const db = await openDatabase(cleanup);
    if (!cleanup) available();
    return new Promise<T>((resolve, reject) => {
      // Resolve only on commit, not request success: returned sources must already be durable.
      const transaction = db.transaction(SOURCE_STORE, mode, {
        durability: "strict",
      });
      const request = action(transaction.objectStore(SOURCE_STORE));
      transaction.oncomplete = () => {
        if (disposed) {
          db.close();
          database = undefined;
        }
        resolve(request.result);
      };
      transaction.onerror = transaction.onabort = () =>
        reject(new Error("Chat file storage failed"));
    });
  }

  function prompt<T>(
    title: string,
    context: FileContext,
    actionLabel: string,
    configure: (content: HTMLElement) => () => T | Promise<T>,
  ): Promise<T | undefined> {
    available();
    if (
      typeof document === "undefined" ||
      !document.body ||
      window.top !== window
    ) {
      return Promise.reject(
        new Error("Trusted Chat file presentation is unavailable"),
      );
    }
    return new Promise<T | undefined>((resolve, reject) => {
      const dialog = document.createElement("dialog");
      const heading = document.createElement("h2");
      heading.textContent = title;
      const summary = document.createElement("p");
      summary.textContent = `Product: ${safeLabel(context.productId)}\nPeer: ${safeLabel(context.peerUsername ?? "Chat contact")}\nIdentity: ${bytesToHex(context.peerIdentity)}`;
      summary.style.whiteSpace = "pre-wrap";
      summary.style.overflowWrap = "anywhere";
      const content = document.createElement("div");
      const accept = document.createElement("button");
      accept.type = "button";
      accept.textContent = actionLabel;
      const cancel = document.createElement("button");
      cancel.type = "button";
      cancel.textContent = "Cancel";
      dialog.setAttribute("aria-label", title);
      dialog.style.maxWidth = "min(36rem, 90vw)";
      dialog.append(heading, summary, content, accept, cancel);
      let settled = false;
      let busy = false;
      const close = () => {
        dialogs.delete(abort);
        dialog.close();
        dialog.remove();
      };
      const abort = () => {
        if (settled) return;
        close();
        // A native save picker cannot be aborted. Let its result reach the caller,
        // which rechecks disposal and aborts the newly-created writable handle.
        if (busy && disposed) return;
        settled = true;
        resolve(undefined);
      };
      dialogs.add(abort);
      cancel.onclick = abort;
      dialog.oncancel = (event) => {
        event.preventDefault();
        if (!busy) abort();
      };
      try {
        const run = configure(content);
        accept.onclick = () => {
          if (busy || settled) return;
          busy = true;
          accept.disabled = cancel.disabled = true;
          // Invoke synchronously in this real click's user activation (FSA requires it).
          let result: T | Promise<T>;
          try {
            result = run();
          } catch (error) {
            result = Promise.reject(error);
          }
          Promise.resolve(result).then(
            (value) => {
              if (!settled) {
                settled = true;
                close();
                resolve(value);
              }
            },
            (error: unknown) => {
              if (settled) return;
              busy = false;
              if (
                error instanceof DOMException &&
                error.name === "AbortError"
              ) {
                abort();
                return;
              }
              settled = true;
              close();
              reject(new Error("Chat file selection or export failed"));
            },
          );
        };
        document.body.append(dialog);
        dialog.showModal();
      } catch {
        settled = true;
        close();
        reject(new Error("Trusted Chat file presentation is unavailable"));
      }
    });
  }

  async function abortExport(id: string, entry: ExportRecord): Promise<void> {
    exports.delete(id);
    try {
      await entry.writer.abort();
    } catch {
      /* The writer may already be closed. */
    }
    await entry.removePartial();
  }

  function withExport(
    id: string,
    action: (entry: ExportRecord) => Promise<void>,
  ): Promise<void> {
    const entry = exports.get(id);
    if (!entry)
      return Promise.reject(new Error("Chat file export is unavailable"));
    const operation = entry.queue.then(async () => {
      if (exports.get(id) !== entry)
        throw new Error("Chat file export is unavailable");
      await action(entry);
    });
    entry.queue = operation.catch(() => {});
    return operation;
  }

  const host: BrowserNativeChatFilesHost = {
    async pickChatFiles(request) {
      checkedSize(request.maxFiles);
      if (request.maxFiles === 0)
        throw new Error("Chat file selection is unavailable");
      // Do not ask for files if durable custody cannot be established.
      if (!sourceStore) await openDatabase();
      const files = await prompt(
        "Send Chat attachments",
        request,
        "Attach files",
        (content) => {
          const label = document.createElement("label");
          label.textContent = `Choose up to ${request.maxFiles} files. The Host keeps a private copy until the transfer is released.`;
          const input = document.createElement("input");
          input.type = "file";
          input.multiple = request.maxFiles > 1;
          label.append(input);
          content.append(label);
          return () => {
            const selected = Array.from(input.files ?? []);
            if (selected.length > request.maxFiles)
              throw new Error("Too many Chat attachments");
            for (const file of selected) checkedSize(file.size);
            return selected;
          };
        },
      );
      if (!files?.length) return [];
      available();
      const picked: NativeChatPickedFile[] = files.map((file) => ({
        sourceId: crypto.randomUUID(),
        metadata: {
          // Initial safe metadata is refined only from the committed immutable Blob.
          mimeType: "application/octet-stream",
          sizeBytes: file.size,
          kind: { tag: "File" },
        },
      }));
      if (sourceStore) {
        await sourceStore.putSources(
          files.map((file, index) => ({
            sourceId: picked[index]!.sourceId,
            blob: file.slice(0, file.size, "application/octet-stream"),
          })),
        );
      } else {
        const db = await openDatabase();
        available();
        await new Promise<void>((resolve, reject) => {
          const transaction = db.transaction(SOURCE_STORE, "readwrite", {
            durability: "strict",
          });
          transaction.oncomplete = () => resolve();
          transaction.onerror = transaction.onabort = () =>
            reject(new Error("Chat file snapshot failed"));
          const sources = transaction.objectStore(SOURCE_STORE);
          try {
            files.forEach((file, index) => {
              // Snapshot source bytes, never source names or paths.
              sources.add(
                {
                  blob: file.slice(0, file.size, "application/octet-stream"),
                } satisfies SourceRecord,
                picked[index]!.sourceId,
              );
            });
          } catch {
            transaction.abort();
          }
        });
      }
      try {
        // Inspect one bounded header/probe at a time, from the durable snapshot
        // rather than the original File, which may since have changed on disk.
        for (const file of picked) {
          if (disposed) break;
          const record = sourceStore
            ? { blob: await sourceStore.readSource(file.sourceId) }
            : await store<SourceRecord | undefined>("readonly", (sources) =>
                sources.get(file.sourceId),
              );
          if (!record || !(record.blob instanceof Blob))
            throw new Error("Chat file snapshot is unavailable");
          file.metadata = await inspectNativeChatFileMetadata(
            record.blob,
            mediaAbort.signal,
          );
        }
        if (!disposed) return picked;
      } catch {
        await Promise.all(
          picked.map((file) => host.releaseChatFile(file.sourceId)),
        );
        if (!disposed) throw new Error("Chat file snapshot inspection failed");
        return [];
      }
      await Promise.all(
        picked.map((file) => host.releaseChatFile(file.sourceId)),
      );
      return [];
    },

    async readChatFile(sourceId, offset, length) {
      checkedSize(length);
      if (
        length > MAX_READ_SIZE ||
        offset < 0n ||
        offset > BigInt(MAX_FILE_SIZE)
      ) {
        throw new Error("Chat file read is out of bounds");
      }
      const record = sourceStore
        ? { blob: await sourceStore.readSource(sourceId) }
        : await store<SourceRecord | undefined>("readonly", (sources) =>
            sources.get(sourceId),
          );
      if (!record || !(record.blob instanceof Blob))
        throw new Error("Chat file source is unavailable");
      checkedSize(record.blob.size);
      if (offset + BigInt(length) > BigInt(record.blob.size))
        throw new Error("Chat file read is out of bounds");
      const start = Number(offset);
      const bytes = new Uint8Array(
        await record.blob.slice(start, start + length).arrayBuffer(),
      );
      if (bytes.byteLength !== length)
        throw new Error("Chat file snapshot read failed");
      return bytes;
    },

    async releaseChatFile(sourceId) {
      // Worker teardown may release a selection that committed after its consumer disappeared.
      if (sourceStore) {
        await sourceStore.releaseSource(sourceId);
        return;
      }
      await store("readwrite", (sources) => sources.delete(sourceId), true);
    },

    async beginChatFileExport(request) {
      available();
      checkedSize(request.metadata.sizeBytes);
      const id = crypto.randomUUID();
      const filename = nativeChatExportFilename(request.metadata);
      const entry = await prompt(
        "Save Chat attachment",
        request,
        "Choose destination",
        (content) => {
          const description = document.createElement("p");
          description.textContent = `${request.metadata.sizeBytes} bytes. Saved as a download, never opened or executed automatically.`;
          content.append(description);
          return async () => {
            const picker = (
              window as Window & { showSaveFilePicker?: SavePicker }
            ).showSaveFilePicker;
            let writer: FileSystemWritableFileStream;
            let removePartial = async () => {};
            let presentCompleted: (() => Promise<void>) | undefined;
            if (picker) {
              const handle = await picker.call(window, {
                suggestedName: filename,
              });
              writer = await handle.createWritable();
            } else {
              if (!navigator.storage?.getDirectory)
                throw new Error("Streaming Chat file export is unavailable");
              const root = await navigator.storage.getDirectory();
              const directory = await root.getDirectoryHandle(
                "truapi-chat-exports",
                { create: true },
              );
              const handle = await directory.getFileHandle(id, {
                create: true,
              });
              removePartial = () => directory.removeEntry(id);
              try {
                writer = await handle.createWritable();
              } catch (error) {
                await removePartial();
                throw error;
              }
              presentCompleted = async () => {
                // getFile supplies a disk-backed snapshot; never concatenate chunks in JS memory.
                const file = await handle.getFile();
                const url = URL.createObjectURL(
                  file.slice(0, file.size, "application/octet-stream"),
                );
                let downloaded = false;
                try {
                  await prompt(
                    "Chat attachment ready",
                    request,
                    "Done",
                    (body) => {
                      const link = document.createElement("a");
                      link.textContent = "Download attachment";
                      link.href = url;
                      link.download = filename;
                      link.onclick = () => {
                        downloaded = true;
                      };
                      body.append(link);
                      return () => undefined;
                    },
                  );
                } finally {
                  URL.revokeObjectURL(url);
                  // Keep an undownloaded completed copy; cancellation must not delete it.
                  if (downloaded) await removePartial();
                }
              };
            }
            const result: ExportRecord = {
              writer,
              size: request.metadata.sizeBytes,
              written: 0,
              queue: Promise.resolve(),
              removePartial,
              presentCompleted,
            };
            if (disposed) {
              try {
                await writer.abort();
              } finally {
                await removePartial();
              }
              return undefined;
            }
            return result;
          };
        },
      );
      if (!entry) return undefined;
      if (disposed) {
        await abortExport(id, entry);
        return undefined;
      }
      exports.set(id, entry);
      return id;
    },

    async writeChatFileExport(exportId, offset, data) {
      available();
      await withExport(exportId, async (entry) => {
        if (
          data.byteLength > MAX_READ_SIZE ||
          offset !== BigInt(entry.written) ||
          BigInt(data.byteLength) + offset > BigInt(entry.size)
        ) {
          throw new Error(
            "Chat file export write is out of bounds or not contiguous",
          );
        }
        try {
          await entry.writer.write(data as FileSystemWriteChunkType);
          entry.written += data.byteLength;
        } catch {
          await abortExport(exportId, entry);
          throw new Error("Chat file export write failed");
        }
      });
    },

    async finishChatFileExport(exportId) {
      available();
      await withExport(exportId, async (entry) => {
        if (entry.written !== entry.size)
          throw new Error("Chat file export is incomplete");
        try {
          await entry.writer.close();
        } catch {
          await abortExport(exportId, entry);
          throw new Error("Chat file export could not be saved");
        }
        // Commit precedes presentation: late cancellation cannot delete a completed user export.
        exports.delete(exportId);
        if (entry.presentCompleted && !disposed) await entry.presentCompleted();
      });
    },

    async cancelChatFileExport(exportId) {
      const entry = exports.get(exportId);
      if (!entry) return;
      const operation = entry.queue.then(async () => {
        if (exports.get(exportId) === entry) await abortExport(exportId, entry);
      });
      entry.queue = operation.catch(() => {});
      await operation;
    },

    dispose() {
      if (disposed) return;
      disposed = true;
      mediaAbort.abort();
      for (const abort of dialogs) abort();
      for (const id of exports.keys())
        void host.cancelChatFileExport(id).catch(() => {});
      const opening = database;
      database = undefined;
      void opening?.then(
        (db) => db.close(),
        () => {},
      );
    },
  };
  return host;
}
