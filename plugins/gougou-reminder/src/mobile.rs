use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::{NotificationTarget, ReminderSchedule, ReminderStatus};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_gougou_reminder);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<GougouReminder<R>> {
    #[cfg(target_os = "android")]
    let handle =
        api.register_android_plugin("com.ym0523lv.gougou.reminder", "GougouReminderPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_gougou_reminder)?;
    Ok(GougouReminder(handle))
}

pub struct GougouReminder<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> GougouReminder<R> {
    pub fn get_status(&self) -> crate::Result<ReminderStatus> {
        self.0
            .run_mobile_plugin("getStatus", ())
            .map_err(Into::into)
    }

    pub fn request_permission(&self) -> crate::Result<ReminderStatus> {
        self.0
            .run_mobile_plugin("requestPermission", ())
            .map_err(Into::into)
    }

    pub fn sync_schedule(&self, schedule: ReminderSchedule) -> crate::Result<ReminderStatus> {
        self.0
            .run_mobile_plugin("syncSchedule", schedule)
            .map_err(Into::into)
    }

    pub fn cancel_all(&self) -> crate::Result<ReminderStatus> {
        self.0
            .run_mobile_plugin("cancelAll", ())
            .map_err(Into::into)
    }

    pub fn take_notification_target(&self) -> crate::Result<NotificationTarget> {
        self.0
            .run_mobile_plugin("takeNotificationTarget", ())
            .map_err(Into::into)
    }
}
