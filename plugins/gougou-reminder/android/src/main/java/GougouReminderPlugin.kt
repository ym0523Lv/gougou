package com.ym0523lv.gougou.reminder

import android.Manifest
import android.app.Activity
import android.content.Intent
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
    private fun status(): JSObject {
        val status = ReminderScheduler.status(activity)
        return JSObject().apply {
            put("supported", true)
            put("permission", status.permission)
            put("exactAlarmAllowed", status.exactAlarmAllowed)
            put("effectivePrecise", status.effectivePrecise)
            put("scheduledCount", status.scheduledCount)
        }
    }

    @Command
    fun getStatus(invoke: Invoke) {
        invoke.resolve(status())
    }

    @Command
    fun requestPermission(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ReminderScheduler.notificationsAllowed(activity)
        ) {
            invoke.resolve(status())
            return
        }
        requestPermissionForAlias("notifications", invoke, "permissionResult")
    }

    @PermissionCallback
    fun permissionResult(invoke: Invoke) {
        invoke.resolve(status())
    }

    @Command
    fun syncSchedule(invoke: Invoke) {
        val args = invoke.parseArgs(ScheduleArgs::class.java)
        val previousPrecise = activity.getSharedPreferences("gougou-reminder", Activity.MODE_PRIVATE)
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
        invoke.resolve(status())
    }

    @Command
    fun cancelAll(invoke: Invoke) {
        ReminderScheduler.cancelAll(activity, clearConfiguration = true)
        invoke.resolve(status())
    }

    @Command
    fun takeNotificationTarget(invoke: Invoke) {
        val date = activity.intent?.getStringExtra("gougouTargetDate")
        activity.intent?.removeExtra("gougouTargetDate")
        invoke.resolve(JSObject().apply { put("targetDate", date) })
    }
}
