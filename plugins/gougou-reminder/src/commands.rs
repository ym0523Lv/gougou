use tauri::{command, AppHandle, Runtime};

use crate::{GougouReminderExt, NotificationTarget, ReminderSchedule, ReminderStatus, Result};

#[command]
pub(crate) async fn get_status<R: Runtime>(app: AppHandle<R>) -> Result<ReminderStatus> {
    app.gougou_reminder().get_status()
}

#[command]
pub(crate) async fn request_permission<R: Runtime>(app: AppHandle<R>) -> Result<ReminderStatus> {
    app.gougou_reminder().request_permission()
}

#[command]
pub(crate) async fn sync_schedule<R: Runtime>(
    app: AppHandle<R>,
    schedule: ReminderSchedule,
) -> Result<ReminderStatus> {
    app.gougou_reminder().sync_schedule(schedule)
}

#[command]
pub(crate) async fn cancel_all<R: Runtime>(app: AppHandle<R>) -> Result<ReminderStatus> {
    app.gougou_reminder().cancel_all()
}

#[command]
pub(crate) async fn take_notification_target<R: Runtime>(
    app: AppHandle<R>,
) -> Result<NotificationTarget> {
    app.gougou_reminder().take_notification_target()
}
