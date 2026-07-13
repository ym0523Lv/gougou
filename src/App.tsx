import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { BackupTestView } from "./BackupTestView";
import { EditorView, type EntryDetail } from "./EditorView";
import { ipcErrorMessage } from "./ipcError";
import { SettingsView } from "./SettingsView";
import type { AppSettings } from "./settings";

type MonthEntrySummary = {
  entryDate: string;
  isTicked: boolean;
  hasContent: boolean;
  updatedAt: number;
};

type CalendarMonth = {
  year: number;
  month: number;
};

type Screen = "calendar" | "editor" | "settings" | "backup-test";

function formatDate(year: number, month: number, day: number) {
  return `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

function localToday() {
  const now = new Date();
  return formatDate(now.getFullYear(), now.getMonth() + 1, now.getDate());
}

function monthKey({ year, month }: CalendarMonth) {
  return `${year}-${String(month).padStart(2, "0")}`;
}

function shiftMonth({ year, month }: CalendarMonth, amount: number): CalendarMonth {
  const date = new Date(year, month - 1 + amount, 1);
  return { year: date.getFullYear(), month: date.getMonth() + 1 };
}

function daysInMonth(year: number, month: number) {
  return new Date(year, month, 0).getDate();
}

function firstWeekdayOffset(year: number, month: number) {
  return (new Date(year, month - 1, 1).getDay() + 6) % 7;
}

function App({
  initialTargetDate,
  settings,
  onSettingsChange,
}: {
  initialTargetDate?: string;
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
}) {
  const today = useMemo(localToday, []);
  const initialDate = initialTargetDate ?? today;
  const [selectedDate, setSelectedDate] = useState(initialDate);
  const [calendarMonth, setCalendarMonth] = useState<CalendarMonth>(() => ({
    year: Number(initialDate.slice(0, 4)),
    month: Number(initialDate.slice(5, 7)),
  }));
  const [entries, setEntries] = useState<Record<string, MonthEntrySummary>>({});
  const [status, setStatus] = useState("正在读取这个月的记录…");
  const [isToggling, setIsToggling] = useState(false);
  const [screen, setScreen] = useState<Screen>("calendar");
  const [drafts, setDrafts] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    setStatus("正在读取这个月的记录…");

    invoke<MonthEntrySummary[]>("get_month_entries", { month: monthKey(calendarMonth) })
      .then((summaries) => {
        if (cancelled) return;
        setEntries(Object.fromEntries(summaries.map((entry) => [entry.entryDate, entry])));
        setStatus("");
      })
      .catch((error) => {
        if (!cancelled) {
          setStatus(ipcErrorMessage(error, "暂时无法读取记录，请稍后重试。"));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [calendarMonth]);

  const days = useMemo(() => {
    const dayCount = daysInMonth(calendarMonth.year, calendarMonth.month);
    const offset = firstWeekdayOffset(calendarMonth.year, calendarMonth.month);
    return Array.from({ length: 42 }, (_, index) => {
      const day = index - offset + 1;
      return day >= 1 && day <= dayCount ? day : null;
    });
  }, [calendarMonth]);

  const selectedEntry = entries[selectedDate];

  async function toggleSelectedDate() {
    setIsToggling(true);
    setStatus("");
    try {
      const entry = await invoke<MonthEntrySummary>("toggle_tick", { date: selectedDate });
      setEntries((current) => ({ ...current, [entry.entryDate]: entry }));
      if (settings.reminder.enabled) {
        void invoke("sync_reminder", { reminder: settings.reminder });
      }
      if (settings.accessibility.haptics && "vibrate" in navigator) navigator.vibrate(15);
    } catch (error) {
      setStatus(ipcErrorMessage(error, "这次没有保存成功，可以再试一次。"));
    } finally {
      setIsToggling(false);
    }
  }

  function updateEntryFromEditor(entry: EntryDetail) {
    setEntries((current) => {
      if (!entry.exists) {
        const next = { ...current };
        delete next[entry.entryDate];
        return next;
      }
      return {
        ...current,
        [entry.entryDate]: {
          entryDate: entry.entryDate,
          isTicked: entry.isTicked,
          hasContent: entry.contentMd.length > 0,
          updatedAt: entry.updatedAt,
        },
      };
    });
  }

  function updateDraft(date: string, content?: string) {
    setDrafts((current) => {
      if (content !== undefined) return { ...current, [date]: content };
      const next = { ...current };
      delete next[date];
      return next;
    });
  }

  function moveCalendarMonth(amount: number) {
    const next = shiftMonth(calendarMonth, amount);
    const selectedDay = Math.min(Number(selectedDate.slice(8)), daysInMonth(next.year, next.month));
    setCalendarMonth(next);
    setSelectedDate(formatDate(next.year, next.month, selectedDay));
  }

  const monthLabel = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "long",
  }).format(new Date(calendarMonth.year, calendarMonth.month - 1, 1));

  if (screen === "editor") {
    return (
      <EditorView
        targetDate={selectedDate}
        initialDraft={drafts[selectedDate]}
        onBack={() => setScreen("calendar")}
        onEntrySaved={updateEntryFromEditor}
        onDraftChange={(content) => updateDraft(selectedDate, content)}
      />
    );
  }

  if (screen === "backup-test" && import.meta.env.DEV) {
    return (
      <BackupTestView
        onBack={() => {
          setScreen("calendar");
          setCalendarMonth((current) => ({ ...current }));
        }}
      />
    );
  }

  if (screen === "settings") {
    return (
      <SettingsView
        onBack={() => setScreen("calendar")}
        onSettingsChange={onSettingsChange}
        settings={settings}
      />
    );
  }

  return (
    <main className="min-h-dvh bg-stone-50 px-5 pb-28 pt-[max(1.5rem,env(safe-area-inset-top))] text-stone-800">
      <header className="mx-auto flex max-w-md items-center justify-between">
        <div>
          <p className="text-sm text-stone-500">勾勾</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-tight">和今天打个招呼</h1>
        </div>
        <div className="flex flex-col items-end gap-2">
          <button className="min-h-11 rounded-xl border border-stone-300 px-3 text-sm font-medium" onClick={() => setScreen("settings")} type="button">设置</button>
          {import.meta.env.DEV && (
            <button
              className="min-h-11 rounded-xl border border-amber-300 bg-amber-50 px-3 text-sm font-medium text-amber-900"
              onClick={() => setScreen("backup-test")}
              type="button"
            >
              备份验收
            </button>
          )}
        </div>
      </header>

      <section aria-label="月历" className="mx-auto mt-10 max-w-md">
        <div className="mb-6 flex items-center justify-between">
          <button
            aria-label="查看上个月"
            className="grid size-11 place-items-center rounded-full text-xl hover:bg-stone-100 focus:outline-none focus:ring-2 focus:ring-emerald-600"
            onClick={() => moveCalendarMonth(-1)}
          >
            ‹
          </button>
          <h2 className="text-lg font-medium">{monthLabel}</h2>
          <button
            aria-label="查看下个月"
            className="grid size-11 place-items-center rounded-full text-xl hover:bg-stone-100 focus:outline-none focus:ring-2 focus:ring-emerald-600"
            onClick={() => moveCalendarMonth(1)}
          >
            ›
          </button>
        </div>

        <div className="grid grid-cols-7 text-center text-xs text-stone-400" aria-hidden="true">
          {["一", "二", "三", "四", "五", "六", "日"].map((day) => (
            <span key={day} className="py-2">{day}</span>
          ))}
        </div>
        <div className="grid grid-cols-7 gap-y-2">
          {days.map((day, index) => {
            if (day === null) return <div key={`blank-${index}`} />;
            const date = formatDate(calendarMonth.year, calendarMonth.month, day);
            const entry = entries[date];
            const selected = date === selectedDate;
            const isToday = date === today;
            return (
              <button
                key={date}
                aria-label={`${date}${entry?.isTicked ? "，已打勾" : ""}${entry?.hasContent ? "，有文字" : ""}`}
                aria-pressed={selected}
                className={`relative mx-auto grid size-11 place-items-center rounded-full text-sm transition focus:outline-none focus:ring-2 focus:ring-emerald-600 ${
                  selected ? "bg-emerald-600 font-medium text-white" : "hover:bg-stone-100"
                }`}
                onClick={() => setSelectedDate(date)}
              >
                {day}
                {entry?.isTicked && <span className={`absolute bottom-1 size-1.5 rounded-full ${selected ? "bg-white" : "bg-emerald-600"}`} />}
                {!entry?.isTicked && entry?.hasContent && <span className="absolute bottom-1 size-1.5 rounded-full bg-amber-500" />}
                {isToday && !selected && <span className="absolute right-1 top-1 size-1 rounded-full bg-stone-400" />}
              </button>
            );
          })}
        </div>
      </section>

      <p className="mx-auto mt-7 max-w-md min-h-6 text-center text-sm text-stone-500" role="status">
        {status}
      </p>

      <section className="fixed inset-x-0 bottom-0 border-t border-stone-200 bg-white/95 px-5 pb-[max(1rem,env(safe-area-inset-bottom))] pt-3 backdrop-blur">
        <div className="mx-auto flex max-w-md gap-3">
          <button
            className="min-h-11 flex-1 rounded-xl bg-emerald-600 px-4 font-medium text-white transition hover:bg-emerald-700 disabled:cursor-wait disabled:opacity-70"
            disabled={isToggling}
            onClick={toggleSelectedDate}
          >
            {selectedEntry?.isTicked ? "已打勾" : "打个勾"}
          </button>
          <button
            className="min-h-11 flex-1 rounded-xl border border-stone-300 px-4 font-medium text-stone-700"
            onClick={() => setScreen("editor")}
          >
            写几句
          </button>
        </div>
      </section>
    </main>
  );
}

export default App;
