package dev.lysen.not

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Countdown between sets. While it runs, a quiet notification counts down in the shade; at the end
 * an alarm (exact, so it's on time with the screen off) posts a buzzing "Rest is over".
 *
 * On Android 16+ the countdown asks to be a Live Update: a ticking chip in the status bar, at the
 * top of the lock screen and in Samsung's Now Bar. Older versions, or a user who turned it off for
 * the app, get the plain ongoing notification.
 */
class RestTimer(private val context: Context) {
    private val prefs = context.getSharedPreferences("rest", Context.MODE_PRIVATE)
    private val _endsAt = MutableStateFlow(prefs.getLong(KEY_ENDS, 0L).takeIf { it > System.currentTimeMillis() })

    /** Wall-clock millis the running countdown ends at; null when stopped. */
    val endsAt: StateFlow<Long?> = _endsAt.asStateFlow()

    private val _duration = MutableStateFlow(prefs.getInt(KEY_DURATION, 90))
    /** Seconds, remembered. */
    val duration: StateFlow<Int> = _duration.asStateFlow()

    private val _startsAfterSet = MutableStateFlow(prefs.getBoolean(KEY_AUTO, true))
    /** Starts by itself after a set is logged. */
    val startsAfterSet: StateFlow<Boolean> = _startsAfterSet.asStateFlow()

    fun setStartsAfterSet(on: Boolean) {
        _startsAfterSet.value = on
        prefs.edit().putBoolean(KEY_AUTO, on).apply()
    }

    fun start(seconds: Int = _duration.value) {
        if (seconds != _duration.value) {
            _duration.value = seconds
            prefs.edit().putInt(KEY_DURATION, seconds).apply()
        }
        runUntil(System.currentTimeMillis() + seconds * 1000L)
    }

    /** Adds (or with a negative value takes off) time from the running countdown. */
    fun extend(seconds: Int) {
        val ends = (_endsAt.value ?: return) + seconds * 1000L
        if (ends <= System.currentTimeMillis()) stop() else runUntil(ends)
    }

    fun stop() {
        alarms().cancel(alarmIntent())
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_RUNNING)
        set(null)
    }

    /** The alarm went off. */
    internal fun finished() {
        set(null)
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_RUNNING)
        notify(
            NOTIFICATION_DONE,
            NotificationCompat.Builder(context, CHANNEL_DONE)
                .setSmallIcon(R.drawable.ic_timer)
                .setContentTitle("Rest is over")
                .setContentText("Time for the next set.")
                .setCategory(NotificationCompat.CATEGORY_ALARM)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .setTimeoutAfter(60_000)
                .build(),
        )
    }

    private fun runUntil(ends: Long) {
        set(ends)
        ensureChannels()
        val alarms = alarms()
        val trigger = SystemClock.elapsedRealtime() + (ends - System.currentTimeMillis())
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms()) {
            alarms.setExactAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, trigger, alarmIntent())
        } else {
            alarms.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, trigger, alarmIntent())
        }
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_DONE)
        notify(
            NOTIFICATION_RUNNING,
            NotificationCompat.Builder(context, CHANNEL_RUNNING)
                .setSmallIcon(R.drawable.ic_timer)
                .setContentTitle("Resting")
                .setContentText("Next set when this runs out")
                .setCategory(NotificationCompat.CATEGORY_STOPWATCH)
                .setRequestPromotedOngoing(true)
                .setUsesChronometer(true)
                .setChronometerCountDown(true)
                .setWhen(ends)
                .setShowWhen(true)
                .setOngoing(true)
                .setSilent(true)
                .setContentIntent(openApp())
                .addAction(0, "+15 s", action(ACTION_PLUS))
                .addAction(0, "Stop", action(ACTION_STOP))
                .setTimeoutAfter(ends - System.currentTimeMillis())
                .build(),
        )
    }

    private fun set(ends: Long?) {
        _endsAt.value = ends
        prefs.edit().putLong(KEY_ENDS, ends ?: 0L).apply()
    }

    private fun notify(id: Int, notification: android.app.Notification) {
        val manager = NotificationManagerCompat.from(context)
        if (manager.areNotificationsEnabled()) {
            try {
                manager.notify(id, notification)
            } catch (_: SecurityException) {
                // Permission withdrawn meanwhile: the in-app countdown still shows.
            }
        }
    }

    private fun ensureChannels() {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_RUNNING, "Rest countdown", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Shows the time left between sets."
                setShowBadge(false)
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_DONE, "Rest over", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Buzzes when it's time for the next set."
                enableVibration(true)
                vibrationPattern = longArrayOf(0, 250, 150, 250)
                setShowBadge(false)
            },
        )
    }

    private fun alarms() = context.getSystemService(AlarmManager::class.java)

    private fun alarmIntent(): PendingIntent =
        PendingIntent.getBroadcast(
            context,
            0,
            Intent(context, RestTimerReceiver::class.java).setAction(ACTION_DONE),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    private fun action(name: String): PendingIntent =
        PendingIntent.getBroadcast(
            context,
            name.hashCode(),
            Intent(context, RestTimerReceiver::class.java).setAction(name),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    private fun openApp(): PendingIntent =
        PendingIntent.getActivity(
            context,
            1,
            MainActivity.openTab(context, Tab.Workouts),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    companion object {
        val presets = listOf(60, 90, 120, 180, 240)

        fun format(seconds: Int): String = "%d:%02d".format(seconds / 60, seconds % 60)

        private const val KEY_ENDS = "ends"
        private const val KEY_DURATION = "duration"
        private const val KEY_AUTO = "after_set"
        private const val CHANNEL_RUNNING = "rest_running"
        private const val CHANNEL_DONE = "rest_done"
        private const val NOTIFICATION_RUNNING = 41
        private const val NOTIFICATION_DONE = 42
        internal const val ACTION_DONE = "dev.lysen.not.REST_DONE"
        internal const val ACTION_PLUS = "dev.lysen.not.REST_PLUS"
        internal const val ACTION_STOP = "dev.lysen.not.REST_STOP"
    }
}

class RestTimerReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val timer = (context.applicationContext as NotApp).restTimer
        when (intent.action) {
            RestTimer.ACTION_DONE -> timer.finished()
            RestTimer.ACTION_PLUS -> timer.extend(15)
            RestTimer.ACTION_STOP -> timer.stop()
        }
    }
}
