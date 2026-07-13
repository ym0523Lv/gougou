import { onBackButtonPress } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { authenticate, checkStatus } from "@tauri-apps/plugin-biometric";
import { useEffect, useRef, useState } from "react";
import { ipcErrorMessage } from "./ipcError";
import { applyPreferences, type AppSettings, type ReminderStatus } from "./settings";

const weekdays = [
  [1, "一"], [2, "二"], [3, "三"], [4, "四"], [5, "五"], [6, "六"], [7, "日"],
] as const;

function localDateAfter(days: number) {
  const date = new Date();
  date.setDate(date.getDate() + days);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function SettingToggle({
  checked,
  disabled,
  label,
  description,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  description: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex min-h-14 items-center justify-between gap-4 py-2">
      <span>
        <span className="block font-medium">{label}</span>
        <span className="mt-1 block text-sm leading-6 text-stone-500">{description}</span>
      </span>
      <input
        aria-label={label}
        checked={checked}
        className="size-5 shrink-0 accent-emerald-700"
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
    </label>
  );
}

export function SettingsView({
  settings,
  onBack,
  onSettingsChange,
}: {
  settings: AppSettings;
  onBack: () => void;
  onSettingsChange: (settings: AppSettings) => void;
}) {
  const [draft, setDraft] = useState(settings);
  const [reminderStatus, setReminderStatus] = useState<ReminderStatus>();
  const [biometricSupported, setBiometricSupported] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const onBackRef = useRef(onBack);
  const persistedRef = useRef(settings);
  onBackRef.current = onBack;

  useEffect(() => {
    let disposed = false;
    let listener: Awaited<ReturnType<typeof onBackButtonPress>> | undefined;
    void onBackButtonPress(() => onBackRef.current()).then((registered) => {
      if (disposed) void registered.unregister();
      else listener = registered;
    });
    return () => {
      disposed = true;
      if (listener) void listener.unregister();
    };
  }, []);

  useEffect(() => {
    void invoke<{ supported: boolean }>("get_biometric_platform_status")
      .then((result) => setBiometricSupported(result.supported))
      .catch(() => setBiometricSupported(false));
  }, []);

  useEffect(() => {
    void invoke<ReminderStatus>("get_reminder_status")
      .then(setReminderStatus)
      .catch(() => setReminderStatus({
        supported: false,
        permission: "unsupported",
        exactAlarmAllowed: false,
        effectivePrecise: false,
        scheduledCount: 0,
      }));
  }, []);

  async function persist(next: AppSettings) {
    const saved = await invoke<AppSettings>("update_app_settings", { settings: next });
    setDraft(saved);
    persistedRef.current = saved;
    applyPreferences(saved);
    onSettingsChange(saved);
    return saved;
  }

  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setStatus("");
    try {
      await action();
    } catch (error) {
      setStatus(ipcErrorMessage(error, "这项设置暂时没有保存。"));
    } finally {
      setBusy(false);
    }
  }

  function updateDraft(next: AppSettings) {
    setDraft(next);
    applyPreferences(next);
  }

  async function setReminderEnabled(enabled: boolean) {
    await run(async () => {
      const next = { ...draft, reminder: { ...draft.reminder, enabled } };
      try {
        if (enabled) {
          const permission = await invoke<ReminderStatus>("request_reminder_permission");
          setReminderStatus(permission);
          if (permission.permission !== "granted") {
            throw new Error("notification permission not granted");
          }
        }
        const synced = await invoke<ReminderStatus>("sync_reminder", { reminder: next.reminder });
        setReminderStatus(synced);
        await persist(next);
        setStatus(enabled ? "晚间提醒已开启。" : "晚间提醒已关闭。");
      } catch (error) {
        void invoke("sync_reminder", { reminder: persistedRef.current.reminder });
        throw error;
      }
    });
  }

  async function saveReminder(next: AppSettings) {
    await run(async () => {
      try {
        if (next.reminder.enabled) {
          const synced = await invoke<ReminderStatus>("sync_reminder", { reminder: next.reminder });
          setReminderStatus(synced);
        }
        await persist(next);
        setStatus("提醒设置已更新。");
      } catch (error) {
        const previous = persistedRef.current;
        setDraft(previous);
        applyPreferences(previous);
        void invoke("sync_reminder", { reminder: previous.reminder });
        throw error;
      }
    });
  }

  async function setPrivacyLock(enabled: boolean) {
    await run(async () => {
      const capability = await checkStatus();
      if (!capability.isAvailable) throw new Error(capability.error ?? "biometric unavailable");
      await authenticate(enabled ? "开启勾勾隐私锁" : "关闭勾勾隐私锁", {
        allowDeviceCredential: true,
        title: enabled ? "开启隐私锁" : "关闭隐私锁",
        confirmationRequired: true,
      });
      await persist({ ...draft, privacy: { lockEnabled: enabled } });
      setStatus(enabled ? "隐私锁已开启。" : "隐私锁已关闭。");
    });
  }

  return (
    <main className="min-h-dvh bg-stone-50 px-5 pb-12 pt-[max(1rem,env(safe-area-inset-top))] text-stone-800">
      <header className="mx-auto flex max-w-xl items-center gap-3">
        <button aria-label="返回月历" className="grid size-11 place-items-center rounded-full text-2xl focus:outline-none focus:ring-2 focus:ring-emerald-600" onClick={onBack} type="button">‹</button>
        <div><p className="text-sm text-stone-500">勾勾</p><h1 className="text-xl font-semibold">设置</h1></div>
      </header>

      <div className="mx-auto mt-7 grid max-w-xl gap-5">
        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">提醒</h2>
          <SettingToggle checked={draft.reminder.enabled} disabled={busy || reminderStatus?.supported === false} label="晚间提醒" description="默认晚上约 10 点，只在你主动开启后使用本地通知。" onChange={(enabled) => void setReminderEnabled(enabled)} />
          <label className="mt-3 block text-sm font-medium" htmlFor="reminder-time">提醒时间</label>
          <input
            id="reminder-time"
            className="mt-2 min-h-11 rounded-xl border border-stone-300 bg-white px-3"
            disabled={busy || !draft.reminder.enabled}
            onChange={(event) => {
              const [hour, minute] = event.target.value.split(":").map(Number);
              updateDraft({ ...draft, reminder: { ...draft.reminder, hour, minute } });
            }}
            onBlur={() => void saveReminder(draft)}
            type="time"
            value={`${String(draft.reminder.hour).padStart(2, "0")}:${String(draft.reminder.minute).padStart(2, "0")}`}
          />
          <SettingToggle checked={draft.reminder.precise} disabled={busy || !draft.reminder.enabled} label="尽量准时" description="Android 可能需要系统特殊授权；未授权时自动使用大约时间。" onChange={(precise) => {
            const next = { ...draft, reminder: { ...draft.reminder, precise } };
            updateDraft(next); void saveReminder(next);
          }} />
          <fieldset className="mt-4" disabled={busy || !draft.reminder.enabled}>
            <legend className="text-sm font-medium">静默日</legend>
            <div className="mt-2 flex flex-wrap gap-2">
              {weekdays.map(([day, label]) => {
                const selected = draft.reminder.quietWeekdays.includes(day);
                return <button aria-pressed={selected} className={`size-11 rounded-full border text-sm ${selected ? "border-emerald-700 bg-emerald-50 text-emerald-800" : "border-stone-300"}`} key={day} onClick={() => {
                  const quietWeekdays = selected ? draft.reminder.quietWeekdays.filter((value) => value !== day) : [...draft.reminder.quietWeekdays, day].sort();
                  const next = { ...draft, reminder: { ...draft.reminder, quietWeekdays } };
                  updateDraft(next); void saveReminder(next);
                }} type="button">{label}</button>;
              })}
            </div>
          </fieldset>
          <div className="mt-4 flex flex-wrap gap-2">
            <button className="min-h-11 rounded-xl border border-stone-300 px-4 disabled:opacity-50" disabled={busy || !draft.reminder.enabled} onClick={() => {
              const next = { ...draft, reminder: { ...draft.reminder, pausedUntil: localDateAfter(7) } };
              updateDraft(next); void saveReminder(next);
            }} type="button">暂停一周</button>
            {draft.reminder.pausedUntil && <button className="min-h-11 rounded-xl border border-stone-300 px-4 disabled:opacity-50" disabled={busy} onClick={() => {
              const next = { ...draft, reminder: { ...draft.reminder, pausedUntil: null } };
              updateDraft(next); void saveReminder(next);
            }} type="button">恢复提醒</button>}
          </div>
          <p className="mt-3 text-sm text-stone-500">{reminderStatus?.supported === false ? "当前平台不支持原生提醒。" : reminderStatus?.permission === "denied" ? "系统通知权限已关闭，可在系统设置中恢复。" : reminderStatus?.effectivePrecise ? "系统已允许尽量准时。" : draft.reminder.precise ? "当前按大约时间提醒。" : "使用大约时间提醒。"}</p>
        </section>

        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">隐私</h2>
          <SettingToggle checked={draft.privacy.lockEnabled} disabled={busy || !biometricSupported} label="隐私锁" description={biometricSupported ? "用系统生物识别或设备凭据进入应用；这不是数据库加密。" : "当前平台不支持系统生物识别锁。"} onChange={(enabled) => void setPrivacyLock(enabled)} />
        </section>

        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">外观与辅助功能</h2>
          <label className="mt-4 block text-sm font-medium" htmlFor="theme">主题</label>
          <select id="theme" className="mt-2 min-h-11 w-full rounded-xl border border-stone-300 bg-white px-3" disabled={busy} value={draft.appearance.theme} onChange={(event) => {
            const theme = event.target.value as AppSettings["appearance"]["theme"];
            const next = { ...draft, appearance: { theme } };
            updateDraft(next); void run(async () => { await persist(next); setStatus("主题已更新。"); });
          }}><option value="system">跟随系统</option><option value="light">浅色</option><option value="dark">深色</option></select>
          <SettingToggle checked={draft.accessibility.reduceMotion} disabled={busy} label="减少动画" description="关闭非必要的过渡动画。" onChange={(reduceMotion) => {
            const next = { ...draft, accessibility: { ...draft.accessibility, reduceMotion } };
            updateDraft(next); void run(async () => { await persist(next); });
          }} />
          <SettingToggle checked={draft.accessibility.haptics} disabled={busy} label="触感反馈" description="打勾成功时提供轻微触感；不支持设备会安静降级。" onChange={(haptics) => {
            const next = { ...draft, accessibility: { ...draft.accessibility, haptics } };
            updateDraft(next); void run(async () => { await persist(next); });
          }} />
        </section>

        <section className="rounded-2xl border border-stone-200 bg-white p-5">
          <h2 className="font-semibold">数据</h2>
          <p className="mt-2 text-sm leading-6 text-stone-500">日记、图片、设置和备份都保存在本机应用沙盒，不会自动上传。</p>
        </section>

        <p className="min-h-6 text-center text-sm text-stone-500" role="status">{busy ? "正在保存…" : status}</p>
      </div>
    </main>
  );
}
