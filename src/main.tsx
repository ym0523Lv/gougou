import { invoke, isTauri } from "@tauri-apps/api/core";
import { authenticate } from "@tauri-apps/plugin-biometric";
import "@fontsource-variable/noto-sans-sc/wght.css";
import React from "react";
import ReactDOM from "react-dom/client";
import { useEffect, useState } from "react";
import App from "./App";
import "./App.css";
import { ipcErrorMessage } from "./ipcError";
import { applyPreferences, type AppSettings } from "./settings";

type HealthStatus = {
  databaseReady: boolean;
  schemaVersion: number;
};

type BiometricPlatformStatus = { supported: boolean };
type NotificationTarget = { targetDate: string | null };
type NotificationNavigation = { targetDate: string; requestId: number };

function BrowserNotice() {
  return (
    <main className="grid min-h-dvh place-items-center bg-stone-50 px-6 text-stone-800">
      <section className="w-full max-w-md rounded-3xl border border-amber-200 bg-white p-7 shadow-sm">
        <p className="text-sm font-medium text-amber-700">当前是浏览器预览</p>
        <h1 className="mt-2 text-2xl font-semibold">这里没有连接本地日记服务</h1>
        <p className="mt-4 leading-7 text-stone-600">
          打勾、正文、图片和备份依赖 Tauri 与本地 SQLite，不能在普通浏览器中使用。
        </p>
        <p className="mt-5 rounded-xl bg-stone-100 p-4 font-mono text-sm">npm run dev</p>
        <p className="mt-3 text-sm text-stone-500">请在命令启动的“勾勾”独立窗口中操作。</p>
      </section>
    </main>
  );
}

function TauriApp() {
  const [health, setHealth] = useState<"checking" | "ready" | "failed">("checking");
  const [error, setError] = useState("");
  const [settings, setSettings] = useState<AppSettings>();
  const [notificationNavigation, setNotificationNavigation] = useState<NotificationNavigation>();
  const [locked, setLocked] = useState(false);
  const [unlocking, setUnlocking] = useState(false);
  const hiddenAt = React.useRef<number | undefined>(undefined);
  const initialized = React.useRef(false);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    Promise.all([
      invoke<HealthStatus>("health_check"),
      invoke<AppSettings>("get_app_settings"),
      invoke<BiometricPlatformStatus>("get_biometric_platform_status"),
      invoke<NotificationTarget>("take_reminder_target").catch(() => ({ targetDate: null })),
      invoke("get_reminder_status").catch(() => undefined),
    ])
      .then(([result, loadedSettings, biometricPlatform, notificationTarget]) => {
        if (!result.databaseReady || result.schemaVersion !== 1) {
          throw new Error("unexpected database health response");
        }
        applyPreferences(loadedSettings);
        setSettings(loadedSettings);
        if (notificationTarget.targetDate) {
          setNotificationNavigation({ targetDate: notificationTarget.targetDate, requestId: 1 });
        }
        setLocked(loadedSettings.privacy.lockEnabled && biometricPlatform.supported);
        setHealth("ready");
      })
      .catch((reason) => {
        setError(ipcErrorMessage(reason, "本地日记服务没有准备好。"));
        setHealth("failed");
      });
  }, []);

  useEffect(() => {
    const updateLockOnVisibility = () => {
      if (document.visibilityState === "hidden") {
        hiddenAt.current = Date.now();
      } else {
        void invoke("get_reminder_status").catch(() => undefined);
        void invoke<NotificationTarget>("take_reminder_target")
          .then((target) => {
            const targetDate = target.targetDate;
            if (!targetDate) return;
            setNotificationNavigation((current) => ({
              targetDate,
              requestId: (current?.requestId ?? 0) + 1,
            }));
          })
          .catch(() => undefined);
        if (
          settings?.privacy.lockEnabled &&
          hiddenAt.current !== undefined &&
          Date.now() - hiddenAt.current >= 30_000
        ) {
          setLocked(true);
        }
      }
    };
    document.addEventListener("visibilitychange", updateLockOnVisibility);
    window.addEventListener("focus", updateLockOnVisibility);
    return () => {
      document.removeEventListener("visibilitychange", updateLockOnVisibility);
      window.removeEventListener("focus", updateLockOnVisibility);
    };
  }, [settings?.privacy.lockEnabled]);

  async function unlock() {
    if (unlocking) return;
    setUnlocking(true);
    setError("");
    try {
      await authenticate("打开勾勾", {
        allowDeviceCredential: true,
        title: "解锁勾勾",
        confirmationRequired: true,
      });
      hiddenAt.current = undefined;
      setLocked(false);
    } catch (reason) {
      setError(ipcErrorMessage(reason, "没有解锁，日记仍然保持隐藏。"));
    } finally {
      setUnlocking(false);
    }
  }

  if (health === "ready" && settings && locked) {
    return (
      <main className="grid min-h-dvh place-items-center bg-stone-50 px-6 text-stone-800">
        <section className="w-full max-w-sm rounded-3xl border border-stone-200 bg-white p-7 text-center shadow-sm">
          <p className="text-sm text-stone-500">勾勾已锁定</p>
          <h1 className="mt-2 text-2xl font-semibold">你的记录仍在本机</h1>
          <p className="mt-4 leading-7 text-stone-600">使用系统生物识别或设备凭据继续。</p>
          {error && <p className="mt-4 text-sm text-amber-800" role="alert">{error}</p>}
          <button className="mt-6 min-h-11 w-full rounded-xl bg-emerald-700 px-5 font-medium text-white disabled:opacity-60" disabled={unlocking} onClick={() => void unlock()} type="button">{unlocking ? "正在验证…" : "解锁"}</button>
        </section>
      </main>
    );
  }

  if (health === "ready" && settings) {
    return <App notificationNavigation={notificationNavigation} settings={settings} onSettingsChange={(next) => {
      applyPreferences(next);
      setSettings(next);
    }} />;
  }

  return (
    <main className="grid min-h-dvh place-items-center bg-stone-50 px-6 text-stone-800">
      <section className="w-full max-w-md rounded-3xl border border-stone-200 bg-white p-7 text-center shadow-sm">
        <h1 className="text-xl font-semibold">{health === "checking" ? "正在打开勾勾…" : "暂时无法打开勾勾"}</h1>
        {health === "failed" && (
          <>
            <p className="mt-4 leading-7 text-stone-600" role="alert">{error}</p>
            <button className="mt-5 min-h-11 rounded-xl bg-stone-800 px-5 text-white" onClick={() => window.location.reload()} type="button">
              重新尝试
            </button>
          </>
        )}
      </section>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isTauri() ? <TauriApp /> : <BrowserNotice />}
  </React.StrictMode>,
);
