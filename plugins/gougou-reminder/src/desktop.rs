use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::{NotificationTarget, ReminderSchedule, ReminderStatus};

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<GougouReminder<R>> {
    Ok(GougouReminder(app.clone()))
}

pub struct GougouReminder<R: Runtime>(AppHandle<R>);

impl<R: Runtime> GougouReminder<R> {
    pub fn get_status(&self) -> crate::Result<ReminderStatus> {
        Ok(ReminderStatus {
            supported: false,
            permission: "unsupported".into(),
            ..Default::default()
        })
    }

    pub fn request_permission(&self) -> crate::Result<ReminderStatus> {
        self.get_status()
    }

    pub fn sync_schedule(&self, _schedule: ReminderSchedule) -> crate::Result<ReminderStatus> {
        self.get_status()
    }

    pub fn cancel_all(&self) -> crate::Result<ReminderStatus> {
        self.get_status()
    }

    pub fn take_notification_target(&self) -> crate::Result<NotificationTarget> {
        Ok(NotificationTarget::default())
    }

    pub fn open_background_settings(&self) -> crate::Result<ReminderStatus> {
        self.get_status()
    }
}
