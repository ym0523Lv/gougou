import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";

type BackupExport = {
  fileName: string;
  sourcePath: string;
  entryCount: number;
  assetCount: number;
};

type BackupPreview = {
  importToken: string;
  entryCount: number;
  assetCount: number;
  conflictCount: number;
};

type BackupImport = {
  importedEntries: number;
};

function errorMessage(error: unknown) {
  if (typeof error === "object" && error !== null) {
    const code = "code" in error ? String(error.code) : "unknown_error";
    const message = "message" in error ? String(error.message) : "操作失败";
    return `${code}: ${message}`;
  }
  return `unknown_error: ${String(error)}`;
}

export function BackupTestView({ onBack }: { onBack: () => void }) {
  const [sourcePath, setSourcePath] = useState("");
  const [preview, setPreview] = useState<BackupPreview>();
  const [replaceConfirmation, setReplaceConfirmation] = useState("");
  const [status, setStatus] = useState("先导出当前数据，或从磁盘选择一个备份包。");
  const [busy, setBusy] = useState(false);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } catch (error) {
      setStatus(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  function changeSource(path: string) {
    setSourcePath(path);
    setPreview(undefined);
    setReplaceConfirmation("");
  }

  async function exportBackup() {
    await run(async () => {
      const result = await invoke<BackupExport>("export_backup");
      changeSource(result.sourcePath);
      setStatus(
        `已导出 ${result.fileName}：${result.entryCount} 个日期，${result.assetCount} 个资源。`,
      );
    });
  }

  async function pickBackup() {
    await run(async () => {
      const selected = await invoke<string | null>("pick_backup_file");
      if (!selected) {
        setStatus("已取消选择。");
        return;
      }
      changeSource(selected);
      setStatus("备份已复制到应用临时目录，可以开始检查。");
    });
  }

  async function inspectBackup() {
    if (!sourcePath.trim()) {
      setStatus("invalid_backup: 请先导出或选择备份包。");
      return;
    }
    await run(async () => {
      const result = await invoke<BackupPreview>("inspect_backup", {
        sourcePath: sourcePath.trim(),
      });
      setPreview(result);
      setReplaceConfirmation("");
      setStatus("检查通过。令牌只能使用一次，并会在 15 分钟后过期。");
    });
  }

  async function saveCopy() {
    if (!sourcePath.trim()) {
      setStatus("invalid_backup: 当前没有可另存的备份包。");
      return;
    }
    await run(async () => {
      const saved = await invoke<boolean>("save_backup_copy", {
        sourcePath: sourcePath.trim(),
      });
      setStatus(saved ? "备份副本已保存到所选位置。" : "已取消另存。");
    });
  }

  async function applyBackup(mode: "merge_newer" | "replace_all") {
    if (!preview) {
      setStatus("invalid_import_token: 请先检查备份包。");
      return;
    }
    if (mode === "replace_all" && replaceConfirmation !== "替换全部") {
      setStatus("请输入“替换全部”后再执行覆盖导入。");
      return;
    }
    await run(async () => {
      const result = await invoke<BackupImport>("apply_backup", {
        importToken: preview.importToken,
        mode,
      });
      setPreview(undefined);
      setReplaceConfirmation("");
      setStatus(`导入完成：写入 ${result.importedEntries} 个日期。请返回月历核对结果。`);
    });
  }

  return (
    <main className="min-h-dvh bg-stone-50 px-5 pb-10 pt-[max(1rem,env(safe-area-inset-top))] text-stone-800">
      <header className="mx-auto flex max-w-xl items-center gap-3">
        <button
          aria-label="返回月历"
          className="grid size-11 place-items-center rounded-full text-2xl focus:outline-none focus:ring-2 focus:ring-emerald-600"
          onClick={onBack}
          type="button"
        >
          ‹
        </button>
        <div>
          <p className="text-xs font-medium uppercase tracking-wide text-amber-700">仅开发模式</p>
          <h1 className="text-xl font-semibold">PHASE 4 备份验收</h1>
        </div>
      </header>

      <div className="mx-auto mt-8 grid max-w-xl gap-5">
        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">1. 准备备份</h2>
          <div className="mt-4 flex flex-wrap gap-3">
            <button className="min-h-11 rounded-xl bg-emerald-700 px-4 text-white disabled:opacity-50" disabled={busy} onClick={() => void exportBackup()} type="button">
              导出当前数据
            </button>
            <button className="min-h-11 rounded-xl border border-stone-300 px-4 disabled:opacity-50" disabled={busy} onClick={() => void pickBackup()} type="button">
              从磁盘选择 ZIP
            </button>
            <button className="min-h-11 rounded-xl border border-stone-300 px-4 disabled:opacity-50" disabled={busy || !sourcePath} onClick={() => void saveCopy()} type="button">
              另存备份副本
            </button>
          </div>
          <label className="mt-5 block text-sm font-medium" htmlFor="backup-source">应用临时路径</label>
          <input
            id="backup-source"
            className="mt-2 min-h-11 w-full rounded-xl border border-stone-300 bg-stone-50 px-3 font-mono text-sm"
            onChange={(event) => changeSource(event.target.value)}
            placeholder="tmp/exports/gougou-backup-YYYYMMDD.zip"
            value={sourcePath}
          />
        </section>

        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">2. 检查</h2>
          <button className="mt-4 min-h-11 rounded-xl bg-stone-800 px-4 text-white disabled:opacity-50" disabled={busy || !sourcePath} onClick={() => void inspectBackup()} type="button">
            验证 ZIP 并生成令牌
          </button>
          {preview && (
            <dl className="mt-5 grid grid-cols-3 gap-3 text-center">
              <div className="rounded-xl bg-stone-100 p-3"><dt className="text-xs text-stone-500">日期</dt><dd className="mt-1 text-lg font-semibold">{preview.entryCount}</dd></div>
              <div className="rounded-xl bg-stone-100 p-3"><dt className="text-xs text-stone-500">资源</dt><dd className="mt-1 text-lg font-semibold">{preview.assetCount}</dd></div>
              <div className="rounded-xl bg-stone-100 p-3"><dt className="text-xs text-stone-500">冲突</dt><dd className="mt-1 text-lg font-semibold">{preview.conflictCount}</dd></div>
            </dl>
          )}
        </section>

        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">3. 应用</h2>
          <button className="mt-4 min-h-11 rounded-xl bg-emerald-700 px-4 text-white disabled:opacity-50" disabled={busy || !preview} onClick={() => void applyBackup("merge_newer")} type="button">
            合并较新内容
          </button>
          <div className="mt-6 border-t border-stone-200 pt-5">
            <label className="block text-sm font-medium" htmlFor="replace-confirmation">危险操作：输入“替换全部”</label>
            <input id="replace-confirmation" className="mt-2 min-h-11 w-full rounded-xl border border-rose-300 px-3" onChange={(event) => setReplaceConfirmation(event.target.value)} value={replaceConfirmation} />
            <button className="mt-3 min-h-11 rounded-xl bg-rose-700 px-4 text-white disabled:opacity-50" disabled={busy || !preview || replaceConfirmation !== "替换全部"} onClick={() => void applyBackup("replace_all")} type="button">
              覆盖全部本地数据
            </button>
          </div>
        </section>

        <p className="min-h-12 rounded-xl bg-amber-50 p-4 text-sm text-amber-950" role="status">
          {busy ? "正在处理，请不要关闭应用…" : status}
        </p>
      </div>
    </main>
  );
}
