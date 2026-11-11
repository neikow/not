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
import androidx.core.content.ContextCompat
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Countdown between sets. While it runs, a quiet notification counts down in the shade; at the end
 * an alarm (exact, so it's on time with the screen off) posts a buzzing "Rest is over".
 *
 * On Android 16+ the countdown asks to be a Live Update: a chip in the status bar, at the top of
 * the lock screen and in Samsung's Now Bar. One UI only shows the chip for a progress-style update
 * with short "critical" text, neither of which ticks by itself, so [RestTimerService] keeps the
 * notification current once a second while resting (and keeps the process awake to do it).
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
        set(null)
        stopTicking()
    }

    /** The alarm went off. */
    internal fun finished() {
        set(null)
        stopTicking()
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
        try {
            ContextCompat.startForegroundService(context, Intent(context, RestTimerService::class.java))
        } catch (_: IllegalStateException) {
            // Not allowed to start from the background here: show it once, without ticking.
            notify(NOTIFICATION_RUNNING, running())
        }
    }

    private fun stopTicking() {
        context.stopService(Intent(context, RestTimerService::class.java))
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_RUNNING)
    }

    /**
     * The countdown as of now: "1:12 left" in the text and the status chip, and a bar filling up as
     * the rest goes by. The time sits in the text, not the header, so it never crowds the title.
     */
    internal fun running(): android.app.Notification {
        ensureChannels()
        val now = System.currentTimeMillis()
        val ends = _endsAt.value ?: now
        val started = prefs.getLong(KEY_STARTED, now).coerceAtMost(now)
        val total = ((ends - started) / 1000).toInt().coerceAtLeast(1)
        val left = ((ends - now + 999) / 1000).toInt().coerceIn(0, total)
        val nextAt = java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(ends))
        return NotificationCompat.Builder(context, CHANNEL_RUNNING)
            .setSmallIcon(R.drawable.ic_timer)
            .setContentTitle("Resting")
            .setContentText("${format(left)} left · next set at $nextAt")
            .setShortCriticalText(format(left))
            .setStyle(
                NotificationCompat.ProgressStyle()
                    .setProgressSegments(listOf(NotificationCompat.ProgressStyle.Segment(total)))
                    .setProgress(total - left)
                    .setStyledByProgress(false),
            )
            .setCategory(NotificationCompat.CATEGORY_STOPWATCH)
            .setRequestPromotedOngoing(true)
            .setShowWhen(false)
            .setOngoing(true)
            .setSilent(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(openApp())
            .addAction(0, "+15 s", action(ACTION_PLUS))
            .addAction(0, "Stop", action(ACTION_STOP))
            .build()
    }

    private fun set(ends: Long?) {
        // A fresh countdown starts the bar from empty; extending keeps where it started.
        val started = if (ends != null && _endsAt.value == null) System.currentTimeMillis() else prefs.getLong(KEY_STARTED, 0L)
        _endsAt.value = ends
        prefs.edit().putLong(KEY_ENDS, ends ?: 0L).putLong(KEY_STARTED, started).apply()
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
        private const val KEY_STARTED = "started"
        private const val KEY_DURATION = "duration"
        private const val KEY_AUTO = "after_set"
        private const val CHANNEL_RUNNING = "rest_running"
        private const val CHANNEL_DONE = "rest_done"
        internal const val NOTIFICATION_RUNNING = 41
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

/**
 * Keeps the rest countdown's notification current, once a second, until it runs out. A foreground
 * service so the process isn't frozen mid-rest; it holds no work of its own and stops with the timer.
 */
class RestTimerService : android.app.Service() {
    private val scope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + kotlinx.coroutines.Dispatchers.Main)
    private var ticker: kotlinx.coroutines.Job? = null

    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val timer = (application as NotApp).restTimer
        val first = timer.running()
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(RestTimer.NOTIFICATION_RUNNING, first, android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(RestTimer.NOTIFICATION_RUNNING, first)
        }
        ticker?.cancel()
        ticker = scope.launch {
            while (true) {
                val ends = timer.endsAt.value ?: break
                if (ends <= System.currentTimeMillis()) break
                NotificationManagerCompat.from(this@RestTimerService).let { manager ->
                    if (manager.areNotificationsEnabled()) {
                        try {
                            manager.notify(RestTimer.NOTIFICATION_RUNNING, timer.running())
                        } catch (_: SecurityException) {
                        }
                    }
                }
                // Land on the next whole second so the shown time steps evenly.
                kotlinx.coroutines.delay(1000 - (ends - System.currentTimeMillis()) % 1000)
            }
            // The alarm posts "Rest is over"; if the process was asleep past it, end here too.
            if ((timer.endsAt.value ?: 0L) <= System.currentTimeMillis()) stopSelf()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
