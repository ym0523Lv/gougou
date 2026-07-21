package com.ym0523lv.gougou.reminder

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class ScheduleArgs {
    var enabled: Boolean = false
    var hour: Int = 22
    var minute: Int = 0
    var precise: Boolean = false
    var quietWeekdays: Array<Int> = emptyArray()
    var pausedUntil: String? = null
    var skipDates: Array<String> = emptyArray()
    var title: String = "勾勾"
    var body: String = "要不要和今天打个招呼？"
}

@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications")
    ]
)
class GougouReminderPlugin(private val activity: Activity) : Plugin(activity) {
    companion object {
        private const val PREFS = "gougou-reminder"
        private const val TARGET_DATE = "gougouTargetDate"
        private const val LAST_EXACT_ALARM_ALLOWED = "lastExactAlarmAllowed"
        private const val LAST_NOTIFICATIONS_ALLOWED = "lastNotificationsAllowed"
    }

    private fun status(
        status: NativeReminderStatus = ReminderScheduler.status(activity),
    ): JSObject {
        return JSObject().apply {
            put("supported", true)
            put("permission", status.permission)
            put("exactAlarmAllowed", status.exactAlarmAllowed)
            put("effectivePrecise", status.effectivePrecise)
            put("scheduledCount", status.scheduledCount)
            put("backgroundSettingsAvailable", backgroundSettingsIntent() != null)
        }
    }

    private fun backgroundSettingsIntent(): Intent? {
        val intent = Intent("com.iqoo.secure.BGSTARTUPMANAGER")
        return intent.takeIf {
            activity.packageManager.resolveActivity(it, PackageManager.MATCH_DEFAULT_ONLY) != null
        }
    }

    private fun reconcileSystemStatus(): NativeReminderStatus {
        val preferences = activity.getSharedPreferences(PREFS, Activity.MODE_PRIVATE)
        val currentStatus = ReminderScheduler.status(activity)
        val notificationsAllowed = currentStatus.permission == "granted"
        val previousNotifications = preferences.getBoolean(
            LAST_NOTIFICATIONS_ALLOWED,
            !notificationsAllowed,
        )
        val precise = preferences.getBoolean("precise", false)
        val previousExactAlarm = preferences.getBoolean(
            LAST_EXACT_ALARM_ALLOWED,
            !currentStatus.exactAlarmAllowed,
        )
        val notificationsChanged = previousNotifications != notificationsAllowed
        val exactAlarmChanged = precise && previousExactAlarm != currentStatus.exactAlarmAllowed
        if (!notificationsChanged && !exactAlarmChanged) return currentStatus

        preferences.edit()
            .putBoolean(LAST_NOTIFICATIONS_ALLOWED, notificationsAllowed)
            .putBoolean(LAST_EXACT_ALARM_ALLOWED, currentStatus.exactAlarmAllowed)
            .apply()
        ReminderScheduler.reschedule(activity)
        return ReminderScheduler.status(activity)
    }

    @Command
    fun getStatus(invoke: Invoke) {
        invoke.resolve(status(reconcileSystemStatus()))
    }

    @Command
    fun requestPermission(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ReminderScheduler.notificationsAllowed(activity)
        ) {
            invoke.resolve(status())
            return
        }
        ReminderScheduler.markNotificationPermissionRequested(activity)
        requestPermissionForAlias("notifications", invoke, "permissionResult")
    }

    @PermissionCallback
    fun permissionResult(invoke: Invoke) {
        invoke.resolve(status())
    }

    @Command
    fun syncSchedule(invoke: Invoke) {
        val args = invoke.parseArgs(ScheduleArgs::class.java)
        val preferences = activity.getSharedPreferences(PREFS, Activity.MODE_PRIVATE)
        val previousPrecise = preferences
            .getBoolean("precise", false)
        if (args.precise && !previousPrecise &&
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
            !activity.getSystemService(android.app.AlarmManager::class.java).canScheduleExactAlarms()
        ) {
            activity.startActivity(
                Intent(
                    Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM,
                    Uri.parse("package:${activity.packageName}"),
                )
            )
        }
        ReminderScheduler.saveAndSchedule(activity, args)
        val currentStatus = ReminderScheduler.status(activity)
        preferences.edit()
            .putBoolean(LAST_EXACT_ALARM_ALLOWED, currentStatus.exactAlarmAllowed)
            .putBoolean(LAST_NOTIFICATIONS_ALLOWED, currentStatus.permission == "granted")
            .apply()
        invoke.resolve(JSObject().apply {
            put("supported", true)
            put("permission", currentStatus.permission)
            put("exactAlarmAllowed", currentStatus.exactAlarmAllowed)
            put("effectivePrecise", currentStatus.effectivePrecise)
            put("scheduledCount", currentStatus.scheduledCount)
        })
    }

    @Command
    fun cancelAll(invoke: Invoke) {
        ReminderScheduler.cancelAll(activity, clearConfiguration = true)
        invoke.resolve(status())
    }

    override fun onNewIntent(intent: Intent) {
        intent.getStringExtra(TARGET_DATE)?.let { date ->
            activity.getSharedPreferences(PREFS, Activity.MODE_PRIVATE)
                .edit().putString(TARGET_DATE, date).apply()
        }
    }

    override fun onResume() {
        reconcileSystemStatus()
    }

    @Command
    fun takeNotificationTarget(invoke: Invoke) {
        val preferences = activity.getSharedPreferences(PREFS, Activity.MODE_PRIVATE)
        val date = preferences.getString(TARGET_DATE, null)
            ?: activity.intent?.getStringExtra(TARGET_DATE)
        preferences.edit().remove(TARGET_DATE).apply()
        activity.intent?.removeExtra(TARGET_DATE)
        invoke.resolve(JSObject().apply { put("targetDate", date) })
    }

    @Command
    fun openBackgroundSettings(invoke: Invoke) {
        backgroundSettingsIntent()?.let(activity::startActivity)
        invoke.resolve(status())
    }
}
