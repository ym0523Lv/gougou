use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::GougouReminder;
#[cfg(mobile)]
use mobile::GougouReminder;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the gougou-reminder APIs.
pub trait GougouReminderExt<R: Runtime> {
    fn gougou_reminder(&self) -> &GougouReminder<R>;
}

impl<R: Runtime, T: Manager<R>> crate::GougouReminderExt<R> for T {
    fn gougou_reminder(&self) -> &GougouReminder<R> {
        self.state::<GougouReminder<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("gougou-reminder")
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::request_permission,
            commands::sync_schedule,
            commands::cancel_all,
            commands::take_notification_target
        ])
        .setup(|app, api| {
            #[cfg(mobile)]
            let gougou_reminder = mobile::init(app, api)?;
            #[cfg(desktop)]
            let gougou_reminder = desktop::init(app, api)?;
            app.manage(gougou_reminder);
            Ok(())
        })
        .build()
}
