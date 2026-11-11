package dev.lysen.not

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dev.lysen.not.core.NotException
import dev.lysen.not.core.TaskItem
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneId

/** What this phone reminds about. Kept per device: the Mac can stay quiet while the phone rings. */
data class ReminderSettings(
    /** A notification when a task with a time is due. */
    val tasks: Boolean = true,
    /** One morning notification listing what's due today. */
    val digest: Boolean = true,
    /** Minutes after midnight. */
    val digestAt: Int = 9 * 60,
    /** An evening nudge naming the habits still to do today. */
    val habits: Boolean = true,
    val habitsAt: Int = 20 * 60,
)

/**
 * Keeps alarms in step with the store: one per task due at a time in the coming week, plus the
 * morning digest and the evening habit nudge. [reschedule] is cheap and idempotent, so it runs after
 * every change, at boot and when the clock or time zone changes.
 */
class Reminders(private val app: NotApp) {
    private val prefs = app.getSharedPreferences("reminders", Context.MODE_PRIVATE)
    private val alarms = app.getSystemService(AlarmManager::class.java)

    private val _settings = MutableStateFlow(load())
    val settings: StateFlow<ReminderSettings> = _settings.asStateFlow()

    init {
        val manager = app.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_TASKS, "Task reminders", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "When a task with a time is due"
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_DIGEST, "Daily summaries", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "What's due in the morning, habits left in the evening"
            },
        )
    }

    fun update(settings: ReminderSettings) {
        _settings.value = settings
        prefs.edit()
            .putBoolean(KEY_TASKS, settings.tasks)
            .putBoolean(KEY_DIGEST, settings.digest)
            .putInt(KEY_DIGEST_AT, settings.digestAt)
            .putBoolean(KEY_HABITS, settings.habits)
            .putInt(KEY_HABITS_AT, settings.habitsAt)
            .apply()
        reschedule()
    }

    val canNotify: Boolean
        get() = Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(app, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    fun reschedule() {
        val s = _settings.value
        val now = LocalDateTime.now()
        val wanted = if (s.tasks) timedTasks(now) else emptyMap()

        // Cancel alarms for tasks that were done, deleted, moved in time or are now too far off.
        val before = prefs.getStringSet(KEY_SCHEDULED, emptySet()).orEmpty()
        for (id in before - wanted.keys) alarms.cancel(taskIntent(id, null))
        for ((id, at) in wanted) setAlarm(at.first, taskIntent(id, at.second))
        prefs.edit().putStringSet(KEY_SCHEDULED, wanted.keys).apply()

        dailyIntent(ACTION_DIGEST).let { if (s.digest) setAlarm(next(now, s.digestAt), it) else alarms.cancel(it) }
        dailyIntent(ACTION_HABITS).let { if (s.habits) setAlarm(next(now, s.habitsAt), it) else alarms.cancel(it) }
    }

    /** Open tasks due at a time in the next week, by id: when, and the title to show. */
    private fun timedTasks(now: LocalDateTime): Map<String, Pair<LocalDateTime, String>> {
        val today = LocalDate.now().toString()
        val tasks = try {
            app.store.dueTasks(today) + app.store.upcomingTasks(today)
        } catch (_: NotException) {
            return emptyMap()
        }
        val horizon = now.plusDays(7)
        return tasks.mapNotNull { task ->
            val at = task.dueAt() ?: return@mapNotNull null
            if (at.isAfter(now) && at.isBefore(horizon)) task.id to (at to task.title) else null
        }.toMap()
    }

    private fun setAlarm(at: LocalDateTime, intent: PendingIntent) {
        val millis = at.atZone(ZoneId.systemDefault()).toInstant().toEpochMilli()
        // USE_EXACT_ALARM is granted at install on 33+; older versions ask for SCHEDULE_EXACT_ALARM.
        if (Build.VERSION.SDK_INT >= 31 && !alarms.canScheduleExactAlarms()) {
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, millis, intent)
        } else {
            alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, millis, intent)
        }
    }

    private fun taskIntent(id: String, title: String?): PendingIntent {
        val intent = Intent(app, ReminderReceiver::class.java)
            .setAction(ACTION_TASK)
            .setData(android.net.Uri.parse("not://task/$id"))
            .putExtra(EXTRA_ID, id)
            .putExtra(EXTRA_TITLE, title)
        return PendingIntent.getBroadcast(app, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }

    private fun dailyIntent(action: String): PendingIntent =
        PendingIntent.getBroadcast(
            app,
            0,
            Intent(app, ReminderReceiver::class.java).setAction(action),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    // ---- what the alarms show ----

    internal fun showTask(id: String, title: String) {
        // Still open? It may have been ticked off on another device since.
        val task = runCatching { app.store.dueTasks(LocalDate.now().toString()) }.getOrNull()?.firstOrNull { it.id == id }
            ?: return
        val time = task.dueTime?.let { "%d:%02d".format(it.toInt() / 60, it.toInt() % 60) }
        val listTitle = runCatching { app.store.lists() }.getOrNull()?.firstOrNull { it.id == task.listId }?.title ?: "Inbox"
        notify(
            id.hashCode(),
            NotificationCompat.Builder(app, CHANNEL_TASKS)
                .setSmallIcon(R.drawable.ic_task_alt)
                .setContentTitle(title)
                .setContentText(listOfNotNull(time, listTitle).joinToString(" · "))
                .setCategory(NotificationCompat.CATEGORY_REMINDER)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(openApp())
                .addAction(0, "Done", actionIntent(ACTION_DONE, id))
                .addAction(0, "In 15 min", actionIntent(ACTION_SNOOZE, id, title))
                .build(),
        )
    }

    internal fun showDigest() {
        val due = runCatching { app.store.dueTasks(LocalDate.now().toString()) }.getOrNull().orEmpty()
        if (due.isNotEmpty()) {
            val overdue = due.count { (it.due ?: "") < LocalDate.now().toString() }
            notify(
                NOTIFICATION_DIGEST,
                NotificationCompat.Builder(app, CHANNEL_DIGEST)
                    .setSmallIcon(R.drawable.ic_today)
                    .setContentTitle(if (due.size == 1) "1 task today" else "${due.size} tasks today")
                    .setContentText(due.joinToString(", ") { it.title })
                    .setStyle(NotificationCompat.InboxStyle().also { style -> due.take(6).forEach { style.addLine(it.title) } }
                        .setSummaryText(if (overdue > 0) "$overdue overdue" else null))
                    .setAutoCancel(true)
                    .setContentIntent(openApp())
                    .build(),
            )
        }
        reschedule()
    }

    internal fun showHabits() {
        val left = runCatching { app.store.habits(LocalDate.now().toString()) }.getOrNull().orEmpty()
            .filter { it.due && !it.done }
        if (left.isNotEmpty()) {
            notify(
                NOTIFICATION_HABITS,
                NotificationCompat.Builder(app, CHANNEL_DIGEST)
                    .setSmallIcon(R.drawable.ic_repeat)
                    .setContentTitle(if (left.size == 1) "1 habit left today" else "${left.size} habits left today")
                    .setContentText(left.joinToString(", ") { it.habit.title })
                    .setAutoCancel(true)
                    .setContentIntent(openApp(tab = Tab.Habits.name))
                    .build(),
            )
        }
        reschedule()
    }

    internal fun complete(id: String) {
        NotificationManagerCompat.from(app).cancel(id.hashCode())
        try {
            app.store.setCompleted(id, true, LocalDate.now().toString())
        } catch (_: NotException) {
            // Gone already: nothing to do.
        }
    }

    internal fun snooze(id: String, title: String) {
        NotificationManagerCompat.from(app).cancel(id.hashCode())
        val intent = PendingIntent.getBroadcast(
            app,
            1,
            Intent(app, ReminderReceiver::class.java)
                .setAction(ACTION_SNOOZED)
                .setData(android.net.Uri.parse("not://snooze/$id"))
                .putExtra(EXTRA_ID, id)
                .putExtra(EXTRA_TITLE, title),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        setAlarm(LocalDateTime.now().plusMinutes(15), intent)
    }

    private fun actionIntent(action: String, id: String, title: String? = null): PendingIntent =
        PendingIntent.getBroadcast(
            app,
            0,
            Intent(app, ReminderReceiver::class.java)
                .setAction(action)
                .setData(android.net.Uri.parse("not://$action/$id"))
                .putExtra(EXTRA_ID, id)
                .putExtra(EXTRA_TITLE, title),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    private fun openApp(tab: String? = null): PendingIntent =
        PendingIntent.getActivity(
            app,
            tab.hashCode(),
            Intent(app, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
                .apply { if (tab != null) putExtra(MainActivity.EXTRA_TAB, tab) },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

    private fun notify(id: Int, notification: android.app.Notification) {
        if (canNotify) NotificationManagerCompat.from(app).notify(id, notification)
    }

    private fun load() = ReminderSettings(
        tasks = prefs.getBoolean(KEY_TASKS, true),
        digest = prefs.getBoolean(KEY_DIGEST, true),
        digestAt = prefs.getInt(KEY_DIGEST_AT, 9 * 60),
        habits = prefs.getBoolean(KEY_HABITS, true),
        habitsAt = prefs.getInt(KEY_HABITS_AT, 20 * 60),
    )

    companion object {
        const val CHANNEL_TASKS = "task_reminders"
        const val CHANNEL_DIGEST = "daily_summaries"
        private const val NOTIFICATION_DIGEST = 2001
        private const val NOTIFICATION_HABITS = 2002

        internal const val ACTION_TASK = "dev.lysen.not.REMIND_TASK"
        internal const val ACTION_DIGEST = "dev.lysen.not.REMIND_DIGEST"
        internal const val ACTION_HABITS = "dev.lysen.not.REMIND_HABITS"
        internal const val ACTION_DONE = "dev.lysen.not.REMIND_DONE"
        internal const val ACTION_SNOOZE = "dev.lysen.not.REMIND_SNOOZE"
        internal const val ACTION_SNOOZED = "dev.lysen.not.REMIND_SNOOZED"
        internal const val EXTRA_ID = "id"
        internal const val EXTRA_TITLE = "title"

        private const val KEY_SCHEDULED = "scheduled"
        private const val KEY_TASKS = "tasks"
        private const val KEY_DIGEST = "digest"
        private const val KEY_DIGEST_AT = "digest_at"
        private const val KEY_HABITS = "habits"
        private const val KEY_HABITS_AT = "habits_at"

        /** The next [minutes]-after-midnight from [now], today or tomorrow. */
        fun next(now: LocalDateTime, minutes: Int): LocalDateTime {
            val today = now.toLocalDate().atTime(LocalTime.of(minutes / 60, minutes % 60))
            return if (today.isAfter(now)) today else today.plusDays(1)
        }
    }
}

private fun TaskItem.dueAt(): LocalDateTime? {
    val day = due?.let { runCatching { LocalDate.parse(it) }.getOrNull() } ?: return null
    val minutes = dueTime?.toInt() ?: return null
    return day.atTime(minutes / 60, minutes % 60)
}

/** Alarms from [Reminders], their notification buttons, and the clock events that move them. */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val reminders = (context.applicationContext as NotApp).reminders
        val id = intent.getStringExtra(Reminders.EXTRA_ID)
        val title = intent.getStringExtra(Reminders.EXTRA_TITLE).orEmpty()
        when (intent.action) {
            Reminders.ACTION_TASK, Reminders.ACTION_SNOOZED -> if (id != null) reminders.showTask(id, title)
            Reminders.ACTION_DIGEST -> reminders.showDigest()
            Reminders.ACTION_HABITS -> reminders.showHabits()
            Reminders.ACTION_DONE -> if (id != null) reminders.complete(id)
            Reminders.ACTION_SNOOZE -> if (id != null) reminders.snooze(id, title)
            // Boot, clock or time zone change, app update: alarms were cleared or are off.
            else -> reminders.reschedule()
        }
    }
}
