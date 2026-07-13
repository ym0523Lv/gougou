package com.ym0523lv.gougou.reminder

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Locale

data class NativeReminderStatus(
    val permission: String,
    val exactAlarmAllowed: Boolean,
    val effectivePrecise: Boolean,
    val scheduledCount: Int,
)

object ReminderScheduler {
    private const val PREFS = "gougou-reminder"
    private const val REQUEST_CODE = 27410
    private const val CHANNEL_ID = "evening-reminder"
    private const val NOTIFICATION_ID = 27411

    fun notificationsAllowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    private fun exactAllowed(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return true
        return context.getSystemService(AlarmManager::class.java).canScheduleExactAlarms()
    }

    fun status(context: Context): NativeReminderStatus {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val precise = prefs.getBoolean("precise", false)
        val exactAllowed = exactAllowed(context)
        return NativeReminderStatus(
            permission = if (notificationsAllowed(context)) "granted" else "prompt",
            exactAlarmAllowed = exactAllowed,
            effectivePrecise = precise && exactAllowed,
            scheduledCount = if (prefs.getBoolean("scheduled", false)) 1 else 0,
        )
    }

    fun saveAndSchedule(context: Context, args: ScheduleArgs) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putBoolean("enabled", args.enabled)
            .putInt("hour", args.hour)
            .putInt("minute", args.minute)
            .putBoolean("precise", args.precise)
            .putStringSet("quietWeekdays", args.quietWeekdays.map(Int::toString).toSet())
            .putString("pausedUntil", args.pausedUntil)
            .putStringSet("skipDates", args.skipDates.toSet())
            .putString("title", args.title)
            .putString("body", args.body)
            .apply()
        cancelAlarm(context)
        scheduleNext(context)
    }

    fun cancelAll(context: Context, clearConfiguration: Boolean) {
        cancelAlarm(context)
        val editor = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putBoolean("scheduled", false)
        if (clearConfiguration) editor.clear()
        editor.apply()
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_ID)
    }

    fun scheduleNext(context: Context) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (!prefs.getBoolean("enabled", false) || !notificationsAllowed(context)) {
            prefs.edit().putBoolean("scheduled", false).apply()
            return
        }
        val hour = prefs.getInt("hour", 22)
        val minute = prefs.getInt("minute", 0)
        val quiet = prefs.getStringSet("quietWeekdays", emptySet()).orEmpty()
        val pausedUntil = prefs.getString("pausedUntil", null)
        val skipDates = prefs.getStringSet("skipDates", emptySet()).orEmpty()
        val formatter = SimpleDateFormat("yyyy-MM-dd", Locale.US)
        val candidate = Calendar.getInstance().apply {
            set(Calendar.HOUR_OF_DAY, hour)
            set(Calendar.MINUTE, minute)
            set(Calendar.SECOND, 0)
            set(Calendar.MILLISECOND, 0)
            if (timeInMillis <= System.currentTimeMillis()) add(Calendar.DAY_OF_MONTH, 1)
        }
        repeat(370) {
            val date = formatter.format(candidate.time)
            val calendarDay = candidate.get(Calendar.DAY_OF_WEEK)
            val isoWeekday = if (calendarDay == Calendar.SUNDAY) 7 else calendarDay - 1
            val paused = pausedUntil != null && date <= pausedUntil
            if (!paused && isoWeekday.toString() !in quiet && date !in skipDates) {
                setAlarm(context, candidate.timeInMillis, date, prefs.getBoolean("precise", false))
                prefs.edit().putBoolean("scheduled", true).apply()
                return
            }
            candidate.add(Calendar.DAY_OF_MONTH, 1)
        }
        prefs.edit().putBoolean("scheduled", false).apply()
    }

    private fun setAlarm(context: Context, triggerAt: Long, date: String, precise: Boolean) {
        val alarm = context.getSystemService(AlarmManager::class.java)
        val pending = alarmPendingIntent(context, date)
        if (precise && exactAllowed(context)) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                alarm.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, triggerAt, pending)
            } else {
                alarm.setExact(AlarmManager.RTC_WAKEUP, triggerAt, pending)
            }
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            alarm.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, triggerAt, pending)
        } else {
            alarm.set(AlarmManager.RTC_WAKEUP, triggerAt, pending)
        }
    }

    private fun cancelAlarm(context: Context) {
        val intent = Intent(context, ReminderReceiver::class.java)
        val pending = PendingIntent.getBroadcast(
            context,
            REQUEST_CODE,
            intent,
            PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE,
        )
        if (pending != null) {
            context.getSystemService(AlarmManager::class.java).cancel(pending)
        }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("scheduled", false).apply()
    }

    private fun alarmPendingIntent(context: Context, date: String): PendingIntent {
        val intent = Intent(context, ReminderReceiver::class.java).putExtra("targetDate", date)
        return PendingIntent.getBroadcast(
            context,
            REQUEST_CODE,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun showNotification(context: Context, date: String) {
        if (!notificationsAllowed(context)) return
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val manager = context.getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "晚间提醒", NotificationManager.IMPORTANCE_DEFAULT)
            )
        }
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?.putExtra("gougouTargetDate", date)
        val contentIntent = launch?.let {
            PendingIntent.getActivity(
                context,
                REQUEST_CODE,
                it,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(context.applicationInfo.icon)
            .setContentTitle(prefs.getString("title", "勾勾"))
            .setContentText(prefs.getString("body", "要不要和今天打个招呼？"))
            .setAutoCancel(true)
            .setContentIntent(contentIntent)
            .build()
        NotificationManagerCompat.from(context).notify(NOTIFICATION_ID, notification)
    }
}
