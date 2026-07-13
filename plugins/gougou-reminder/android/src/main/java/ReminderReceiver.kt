package com.ym0523lv.gougou.reminder

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val date = intent.getStringExtra("targetDate") ?: return
        ReminderScheduler.showNotification(context, date)
        ReminderScheduler.scheduleNext(context)
    }
}
