import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { onBackButtonPress } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { EditorContent, useEditor } from "@tiptap/react";
import Image from "@tiptap/extension-image";
import { mergeAttributes } from "@tiptap/core";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipcErrorMessage } from "./ipcError";

export type EntryDetail = {
  exists: boolean;
  entryDate: string;
  contentMd: string;
  wordCount: number;
  revision: number;
  isTicked: boolean;
  updatedAt: number;
};

type AssetDetail = {
  assetName: string;
  previewUrl: string;
  mimeType: string;
  width: number;
  height: number;
};

type SaveState = "loading" | "saved" | "unsaved" | "saving" | "failed";

type EditorViewProps = {
  targetDate: string;
  initialDraft?: string;
  onBack: () => void;
  onEntrySaved: (entry: EntryDetail) => void;
  onDraftChange: (content?: string) => void;
};

const AssetImage = Image.extend({
  renderHTML({ HTMLAttributes }) {
    const source = String(HTMLAttributes.src ?? "");
    const previewSource = source.startsWith("assets/")
      ? convertFileSrc(source.slice("assets/".length), "gougou-asset")
      : source;
    return ["img", mergeAttributes(HTMLAttributes, { src: previewSource })];
  },
});

const editorExtensions = [
  StarterKit.configure({
    heading: { levels: [1, 2, 3] },
  }),
  TaskList,
  TaskItem.configure({ nested: true }),
  AssetImage,
  Markdown,
];

function saveLabel(state: SaveState) {
  switch (state) {
    case "loading":
      return "正在打开…";
    case "unsaved":
      return "尚未保存";
    case "saving":
      return "正在保存…";
    case "failed":
      return "暂未保存";
    default:
      return "已保存";
  }
}

function formatDateLabel(date: string) {
  const [year, month, day] = date.split("-").map(Number);
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "long",
    day: "numeric",
    weekday: "short",
  }).format(new Date(year, month - 1, day));
}

function ToolButton({
  label,
  active = false,
  onClick,
}: {
  label: string;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      aria-pressed={active}
      className={`grid min-h-11 min-w-11 w-max flex-none place-items-center whitespace-nowrap rounded-lg px-1 text-[clamp(0.75rem,3.5vw,0.875rem)] font-medium focus:outline-none focus:ring-2 focus:ring-emerald-600 ${
        active ? "bg-emerald-100 text-emerald-800" : "text-stone-700 hover:bg-stone-100"
      }`}
      onPointerDown={(event) => event.preventDefault()}
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}

export function EditorView({
  targetDate,
  initialDraft,
  onBack,
  onEntrySaved,
  onDraftChange,
}: EditorViewProps) {
  const [saveState, setSaveState] = useState<SaveState>("loading");
  const [loadError, setLoadError] = useState(false);
  const [operationError, setOperationError] = useState("");
  const [keyboardOffset, setKeyboardOffset] = useState(0);
  const revisionRef = useRef(0);
  const latestContentRef = useRef("");
  const confirmedContentRef = useRef("");
  const saveTimerRef = useRef<number | undefined>();
  const savingRef = useRef(false);
  const activeSaveRef = useRef<Promise<void> | null>(null);
  const latestRequestRef = useRef(0);
  const flushRef = useRef<() => Promise<void>>(async () => undefined);
  const scheduleSaveRef = useRef<(content: string) => void>(() => undefined);
  const initialDraftRef = useRef(initialDraft);
  const onBackRef = useRef(onBack);
  const returningRef = useRef(false);

  onBackRef.current = onBack;

  const editor = useEditor({
    extensions: editorExtensions,
    content: "",
    contentType: "markdown",
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: "editor-content min-h-[50dvh] outline-none",
        "aria-label": "日记内容",
      },
    },
    onUpdate: ({ editor: currentEditor }) => {
      scheduleSaveRef.current(currentEditor.getMarkdown());
    },
  });

  const loadEntry = useCallback(async () => {
    setSaveState("loading");
    setLoadError(false);
    setOperationError("");
    try {
      const detail = await invoke<EntryDetail>("get_entry_detail", { date: targetDate });
      const content = initialDraftRef.current ?? detail.contentMd;
      revisionRef.current = detail.revision;
      latestContentRef.current = content;
      confirmedContentRef.current = detail.contentMd;
      editor?.commands.setContent(content, { contentType: "markdown", emitUpdate: false });
      setSaveState(content === detail.contentMd ? "saved" : "unsaved");
      if (content !== detail.contentMd) scheduleSaveRef.current(content);
    } catch (error) {
      setLoadError(true);
      setOperationError(ipcErrorMessage(error, "这篇记录暂时没有打开。"));
      setSaveState("failed");
    }
  }, [editor, targetDate]);

  useEffect(() => {
    void loadEntry();
  }, [loadEntry]);

  const flush = useCallback(async () => {
    if (saveTimerRef.current !== undefined) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = undefined;
    }
    if (loadError || latestContentRef.current === confirmedContentRef.current) return;
    if (savingRef.current) {
      await activeSaveRef.current;
      if (latestContentRef.current !== confirmedContentRef.current) {
        await flushRef.current();
      }
      return;
    }

    const contentToSave = latestContentRef.current;
    const expectedRevision = revisionRef.current;
    const requestId = ++latestRequestRef.current;
    savingRef.current = true;
    setSaveState("saving");
    const activeSave = (async () => {
      try {
        const detail = await invoke<EntryDetail>("save_entry", {
          date: targetDate,
          contentMd: contentToSave,
          expectedRevision,
        });
        if (requestId === latestRequestRef.current) {
          revisionRef.current = detail.revision;
          confirmedContentRef.current = contentToSave;
          const hasNewerContent = latestContentRef.current !== contentToSave;
          if (!hasNewerContent) onDraftChange();
          onEntrySaved(detail);
          setOperationError("");
          setSaveState(hasNewerContent ? "unsaved" : "saved");
        }
      } catch (error) {
        if (requestId === latestRequestRef.current) {
          setOperationError(ipcErrorMessage(error, "这次没有保存成功，可以再试一次。"));
          setSaveState("failed");
        }
      } finally {
        savingRef.current = false;
      }
    })();
    activeSaveRef.current = activeSave;
    await activeSave;
    if (activeSaveRef.current === activeSave) activeSaveRef.current = null;
  }, [loadError, onDraftChange, onEntrySaved, targetDate]);

  flushRef.current = flush;
  scheduleSaveRef.current = (content) => {
    latestContentRef.current = content;
    onDraftChange(content);
    if (saveTimerRef.current !== undefined) window.clearTimeout(saveTimerRef.current);
    setOperationError("");
    setSaveState("unsaved");
    saveTimerRef.current = window.setTimeout(() => {
      void flushRef.current();
    }, 1500);
  };

  useEffect(() => {
    const flushWhenHidden = () => {
      if (document.visibilityState === "hidden") void flushRef.current();
    };
    const flushOnPageHide = () => void flushRef.current();
    document.addEventListener("visibilitychange", flushWhenHidden);
    window.addEventListener("pagehide", flushOnPageHide);
    return () => {
      document.removeEventListener("visibilitychange", flushWhenHidden);
      window.removeEventListener("pagehide", flushOnPageHide);
      void flushRef.current();
    };
  }, []);

  useEffect(() => {
    const viewport = window.visualViewport;
    if (!viewport) return;
    const updateKeyboardOffset = () => {
      setKeyboardOffset(Math.max(0, window.innerHeight - viewport.height - viewport.offsetTop));
    };
    updateKeyboardOffset();
    viewport.addEventListener("resize", updateKeyboardOffset);
    viewport.addEventListener("scroll", updateKeyboardOffset);
    return () => {
      viewport.removeEventListener("resize", updateKeyboardOffset);
      viewport.removeEventListener("scroll", updateKeyboardOffset);
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let listener: Awaited<ReturnType<typeof onBackButtonPress>> | undefined;
    void onBackButtonPress(() => {
      void returnToCalendar();
    }).then((registered) => {
      if (disposed) {
        void registered.unregister();
      } else {
        listener = registered;
      }
    });
    return () => {
      disposed = true;
      if (listener) void listener.unregister();
    };
  }, []);

  async function returnToCalendar() {
    if (returningRef.current) return;
    returningRef.current = true;
    try {
      await flushRef.current();
      onBackRef.current();
    } finally {
      returningRef.current = false;
    }
  }

  async function insertImage() {
    try {
      let asset: AssetDetail | null;
      try {
        asset = await invoke<AssetDetail | null>("pick_and_import_image");
      } catch (error) {
        if (
          typeof error !== "object" ||
          error === null ||
          !("code" in error) ||
          error.code !== "unsupported_platform"
        ) {
          throw error;
        }
        const sourcePath = await open({
          multiple: false,
          pickerMode: "image",
          fileAccessMode: "copy",
          filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
        });
        if (!sourcePath || Array.isArray(sourcePath)) return;
        asset = await invoke<AssetDetail>("import_image", { sourcePath });
      }
      if (!asset) return;
      editor?.chain().focus().setImage({ src: asset.assetName }).run();
      setOperationError("");
    } catch (error) {
      setOperationError(ipcErrorMessage(error, "图片暂时没有插入。"));
      setSaveState("failed");
    }
  }

  return (
    <main className="min-h-[100dvh] bg-stone-50 pb-24 text-stone-800">
      <header className="sticky top-0 z-10 grid min-h-16 grid-cols-[2.75rem_minmax(0,1fr)_2.75rem] items-center border-b border-stone-200 bg-stone-50/95 px-4 pb-2 pt-[max(1.5rem,env(safe-area-inset-top))] backdrop-blur">
        <button
          aria-label="返回月历"
          className="grid min-h-11 min-w-11 place-items-center rounded-full text-2xl focus:outline-none focus:ring-2 focus:ring-emerald-600"
          onClick={() => void returnToCalendar()}
          type="button"
        >
          ‹
        </button>
        <div className="min-w-0 text-center">
          <p className="break-words text-sm font-medium leading-tight">{formatDateLabel(targetDate)}</p>
          <p className={`text-xs ${saveState === "failed" ? "text-amber-700" : "text-stone-500"}`} role="status">
            {saveLabel(saveState)}
          </p>
        </div>
        <div className="min-w-11" aria-hidden="true" />
      </header>

      <section className="mx-auto max-w-2xl px-5 py-8">
        {loadError ? (
          <div className="rounded-2xl bg-amber-50 p-5 text-amber-900">
            <p>{operationError || "这篇记录暂时没有打开。"}</p>
            <button className="mt-3 min-h-11 rounded-lg bg-amber-900 px-4 text-white" onClick={() => void loadEntry()} type="button">
              重试
            </button>
          </div>
        ) : (
          <>
            <EditorContent editor={editor} />
            {saveState === "failed" && (
              <div className="mt-5 text-amber-800">
                {operationError && <p className="text-sm" role="alert">{operationError}</p>}
                <button
                  className="mt-3 min-h-11 rounded-lg border border-amber-700 px-4"
                  onClick={() => void flushRef.current()}
                  type="button"
                >
                  重试保存
                </button>
              </div>
            )}
          </>
        )}
      </section>

      <nav
        aria-label="编辑格式"
        className="fixed inset-x-0 z-20 border-t border-stone-200 bg-white/95 px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] pt-2 backdrop-blur"
        style={{ bottom: `max(${keyboardOffset}px, var(--keyboard-inset-height, 0px))` }}
      >
        <div className="mx-auto flex max-w-2xl gap-0.5 overflow-x-auto">
          <ToolButton active={editor?.isActive("bold")} label="粗体" onClick={() => editor?.chain().focus().toggleBold().run()} />
          <ToolButton active={editor?.isActive("heading", { level: 2 })} label="标题" onClick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()} />
          <ToolButton active={editor?.isActive("bulletList")} label="列表" onClick={() => editor?.chain().focus().toggleBulletList().run()} />
          <ToolButton active={editor?.isActive("taskList")} label="待办" onClick={() => editor?.chain().focus().toggleTaskList().run()} />
          <ToolButton label="图片" onClick={() => void insertImage()} />
          <ToolButton label="撤销" onClick={() => editor?.chain().focus().undo().run()} />
          <ToolButton label="重做" onClick={() => editor?.chain().focus().redo().run()} />
        </div>
      </nav>
    </main>
  );
}
