export type ReminderSettings = {
  enabled: boolean;
  hour: number;
  minute: number;
  precise: boolean;
  quietWeekdays: number[];
  pausedUntil: string | null;
};

export type AppSettings = {
  reminder: ReminderSettings;
  privacy: { lockEnabled: boolean };
  appearance: { theme: "system" | "light" | "dark" };
  accessibility: { reduceMotion: boolean; haptics: boolean };
};

export type ReminderStatus = {
  supported: boolean;
  permission: "granted" | "denied" | "prompt" | "unsupported";
  exactAlarmAllowed: boolean;
  effectivePrecise: boolean;
  scheduledCount: number;
  backgroundSettingsAvailable: boolean;
};

export function applyPreferences(settings: AppSettings) {
  document.documentElement.dataset.theme = settings.appearance.theme;
  document.documentElement.dataset.reduceMotion = String(settings.accessibility.reduceMotion);
  document.documentElement.style.colorScheme =
    settings.appearance.theme === "system" ? "light dark" : settings.appearance.theme;
}
