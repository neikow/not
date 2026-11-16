package dev.lysen.not

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.NotException
import androidx.glance.appwidget.updateAll
import kotlinx.coroutines.launch
import java.time.LocalDate

/**
 * What timed habits show outside the app, on this phone:
 *
 * - a **live notification** (lock screen, Now Bar chip) while a habit is out of its usual state —
 *   paused when it's normally on, running when it's normally off — if switched on for that habit
 *   here. Its chronometer counts the stretch by itself, so nothing ticks in the background.
 * - a **nudge** once a stretch out of the usual state reaches the habit's reminder delay.
 *
 * [refresh] is cheap and idempotent: it runs after every change (local or synced) and at start.
 */
class TimedAlerts(private val app: NotApp) {
    private val prefs = app.getSharedPreferences("timed", Context.MODE_PRIVATE)
    private val alarms = app.getSystemService(AlarmManager::class.java)

    init {
        val manager = app.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_LIVE, "Timed habits", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Shows a timed habit while it's paused or running, with a button to switch it"
                setShowBadge(false)
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_NUDGE, "Timed habit reminders", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "When a timed habit has been paused (or running) for its reminder time"
            },
        )
    }

    /** Whether [habitId] shows a live notification on this phone. Off by default. */
    fun liveEnabled(habitId: String): Boolean = prefs.getBoolean(KEY_LIVE + habitId, false)

    fun setLive(habitId: String, on: Boolean) {
        prefs.edit().putBoolean(KEY_LIVE + habitId, on).apply()
        refresh()
    }

    fun refresh() {
        val habits = try {
            app.store.habitsNow(LocalDate.now().toString()).filter { it.habit.timed != null }
        } catch (_: NotException) {
            return
        }
        val now = nowMs()
        val shown = prefs.getStringSet(KEY_SHOWN, emptySet()).orEmpty()
        val nudging = prefs.getStringSet(KEY_NUDGES, emptySet()).orEmpty()
        val wantShown = mutableSetOf<String>()
        val wantNudge = mutableSetOf<String>()
        // Rings on the widget move while time counts toward a goal: redraw them now and then,
        // without waking the phone (the number in the ring ticks by itself).
        val counting = habits.any { s ->
            val rule = s.habit.timed
            val state = s.timed
            rule != null && state?.since != null && state.on != rule.atMostOff
        }
        // Next wake-up: the 5-minute redraw while counting, or sooner when a shown notification's
        // goal becomes safe (it goes away then).
        var nextTick = if (counting) now + TICK_MS else Long.MAX_VALUE
        for (summary in habits) {
            val id = summary.habit.id
            val rule = summary.habit.timed ?: continue
            val state = summary.timed ?: continue
            val since = state.since ?: continue
            if (state.on == rule.usualOn) continue
            val timed = summary.timedLive(now, now)!!
            // Once today's goal can't be missed, the notification has done its job; the stretch
            // keeps counting in the app and widget.
            if (liveEnabled(id) && !timed.secured(now)) {
                wantShown += id
                notify(liveId(id), live(summary, now))
                timed.securedAt(now)?.let { nextTick = minOf(nextTick, it) }
            }
            if (rule.nudgeMinutes > 0u) {
                val at = since + rule.nudgeMinutes.toLong() * 60_000
                // Once per stretch: a nudge already shown for this one isn't repeated.
                if (prefs.getLong(KEY_NUDGED + id, 0L) != since && at > now - 60_000) {
                    wantNudge += id
                    setAlarm(at, nudgeIntent(id, since))
                }
            }
        }
        val tick = tickIntent()
        if (nextTick == Long.MAX_VALUE) alarms.cancel(tick) else alarms.setExact(AlarmManager.RTC, nextTick, tick)
        for (id in shown - wantShown) NotificationManagerCompat.from(app).cancel(liveId(id))
        for (id in nudging - wantNudge) alarms.cancel(nudgeIntent(id, 0L))
        // A nudge whose stretch ended has nothing left to say.
        for (summary in habits) {
            val rule = summary.habit.timed ?: continue
            if (summary.timed?.on == rule.usualOn) NotificationManagerCompat.from(app).cancel(nudgeId(summary.habit.id))
        }
        prefs.edit().putStringSet(KEY_SHOWN, wantShown).putStringSet(KEY_NUDGES, wantNudge).apply()
    }

    /** From the nudge alarm: still out of the usual state, same stretch? Then say so. */
    internal fun nudge(habitId: String, since: Long) {
        val summary = try {
            app.store.habitsNow(LocalDate.now().toString()).firstOrNull { it.habit.id == habitId }
        } catch (_: NotException) {
            null
        } ?: return
        val rule = summary.habit.timed ?: return
        val state = summary.timed ?: return
        if (state.since != since || state.on == rule.usualOn) return
        prefs.edit().putLong(KEY_NUDGED + habitId, since).apply()
        val stretch = durationText(nowMs() - since)
        val title = if (rule.usualOn) "${summary.habit.title} paused for $stretch" else "${summary.habit.title} running for $stretch"
        val text = if (rule.usualOn) "Resume it when it's back on." else "Stop it if you're done."
        notify(
            nudgeId(habitId),
            NotificationCompat.Builder(app, CHANNEL_NUDGE)
                .setSmallIcon(R.drawable.ic_timer)
                .setContentTitle(title)
                .setContentText(text)
                .setCategory(NotificationCompat.CATEGORY_REMINDER)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(openHabits())
                .addAction(0, switchLabel(rule, state.on), switchIntent(habitId, rule.usualOn))
                .build(),
        )
    }

    /** From a notification button: back to the usual state now. */
    internal fun switchTo(habitId: String, on: Boolean) {
        val now = nowMs()
        try {
            app.store.setHabitState(habitId, on, now, null, now)
        } catch (_: NotException) {
            // Deleted elsewhere meanwhile.
        }
        NotificationManagerCompat.from(app).cancel(nudgeId(habitId))
        refresh()
        SyncWorker.pushSoon(app)
    }

    /**
     * "Paused since 14:20 · 1h 35m left, until 16:05": everything in it stays true while it sits
     * there; the chronometer in the header (and the Now Bar chip) counts the stretch.
     */
    private fun live(summary: HabitSummary, now: Long): android.app.Notification {
        val rule = summary.habit.timed!!
        val state = summary.timed!!
        val since = state.since ?: now
        val live = summary.timedLive(now, now)!!
        val head = "${stateName(rule, state.on)} since ${clockText(since)}"
        // The running stretch moves the goal or eats the allowance at one minute per minute.
        val counts = state.on != rule.atMostOff
        // Once today's goal is safe, the notification stops talking about it.
        val tail = when {
            live.secured(now) -> null
            live.atMostOff && live.leftMs > 0 && counts -> "${durationText(live.leftMs)} left, until ${clockText(now + live.leftMs)}"
            live.atMostOff && live.leftMs <= 0 -> "${durationText(-live.leftMs)} over today"
            !live.atMostOff && counts -> "goal at ${clockText(now + live.leftMs)}"
            else -> null
        }
        return NotificationCompat.Builder(app, CHANNEL_LIVE)
            .setSmallIcon(R.drawable.ic_timer)
            .setContentTitle(summary.habit.title)
            .setContentText(listOfNotNull(head, tail).joinToString(" · "))
            .setWhen(since)
            .setShowWhen(true)
            .setUsesChronometer(true)
            .setCategory(NotificationCompat.CATEGORY_STOPWATCH)
            .setRequestPromotedOngoing(true)
            .setOngoing(true)
            .setSilent(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(openHabits())
            .addAction(0, switchLabel(rule, state.on), switchIntent(summary.habit.id, !state.on))
            .build()
    }

    private fun notify(id: Int, notification: android.app.Notification) {
        if (!app.reminders.canNotify) return
        try {
            NotificationManagerCompat.from(app).notify(id, notification)
        } catch (_: SecurityException) {
        }
    }

    private fun setAlarm(at: Long, intent: PendingIntent) {
        if (Build.VERSION.SDK_INT >= 31 && !alarms.canScheduleExactAlarms()) {
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
        } else {
            alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
        }
    }

    // One request code per habit, so a habit's alarm or button replaces its previous one.
    private fun nudgeIntent(habitId: String, since: Long): PendingIntent = PendingIntent.getBroadcast(
        app,
        nudgeId(habitId),
        Intent(app, TimedReceiver::class.java).setAction(ACTION_NUDGE).putExtra(EXTRA_ID, habitId).putExtra(EXTRA_SINCE, since),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun switchIntent(habitId: String, on: Boolean): PendingIntent = PendingIntent.getBroadcast(
        app,
        liveId(habitId),
        Intent(app, TimedReceiver::class.java).setAction(ACTION_SWITCH).putExtra(EXTRA_ID, habitId).putExtra(EXTRA_ON, on),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun openHabits(): PendingIntent = PendingIntent.getActivity(
        app,
        REQUEST_OPEN,
        MainActivity.openTab(app, Tab.Habits).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun tickIntent(): PendingIntent = PendingIntent.getBroadcast(
        app,
        REQUEST_TICK,
        Intent(app, TimedReceiver::class.java).setAction(ACTION_TICK),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    /** From the tick: redraw the widgets, which schedules the next tick if still counting. */
    internal fun tick(done: () -> Unit) {
        kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.Dispatchers.Default).launch {
            try {
                dev.lysen.not.widget.TimedWidget().updateAll(app)
                refresh()
            } finally {
                done()
            }
        }
    }

    private fun liveId(habitId: String) = 0x7100_0000 or (habitId.hashCode() and 0xFFFFFF)

    private fun nudgeId(habitId: String) = 0x7200_0000 or (habitId.hashCode() and 0xFFFFFF)

    companion object {
        const val CHANNEL_LIVE = "timed_live"
        const val CHANNEL_NUDGE = "timed_nudge"
        internal const val ACTION_SWITCH = "dev.lysen.not.TIMED_SWITCH"
        internal const val ACTION_NUDGE = "dev.lysen.not.TIMED_NUDGE"
        internal const val ACTION_TICK = "dev.lysen.not.TIMED_TICK"
        private const val TICK_MS = 5 * 60_000L
        private const val REQUEST_TICK = 0x7400_0000
        internal const val EXTRA_ID = "id"
        internal const val EXTRA_ON = "on"
        internal const val EXTRA_SINCE = "since"
        private const val REQUEST_OPEN = 0x7300_0000
        private const val KEY_LIVE = "live_"
        private const val KEY_NUDGED = "nudged_"
        private const val KEY_SHOWN = "shown"
        private const val KEY_NUDGES = "nudges"
    }
}

class TimedReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val alerts = (context.applicationContext as NotApp).timedAlerts
        if (intent.action == TimedAlerts.ACTION_TICK) {
            val pending = goAsync()
            alerts.tick { pending.finish() }
            return
        }
        val id = intent.getStringExtra(TimedAlerts.EXTRA_ID) ?: return
        when (intent.action) {
            TimedAlerts.ACTION_SWITCH -> alerts.switchTo(id, intent.getBooleanExtra(TimedAlerts.EXTRA_ON, true))
            TimedAlerts.ACTION_NUDGE -> alerts.nudge(id, intent.getLongExtra(TimedAlerts.EXTRA_SINCE, 0L))
        }
    }
}
