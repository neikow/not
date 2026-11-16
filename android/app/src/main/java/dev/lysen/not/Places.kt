package dev.lysen.not

import android.Manifest
import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dev.lysen.not.core.TaskItem
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

/** A spot the user saved ("Home", "Gym") to be reminded at. */
data class Place(val id: String, val name: String, val lat: Double, val lon: Double, val radius: Float = 150f)

/** Remind about [taskId] on arriving at (or leaving) [placeId]. */
data class PlaceReminder(val taskId: String, val placeId: String, val arriving: Boolean)

/**
 * Location reminders, kept on this phone only (like time reminders): saved places, which task to
 * remind about where, and the system proximity alerts behind them. Uses the platform's
 * [LocationManager], so it works without Google Play services. Alerts are one-shot: once a reminder
 * fires it is cleared.
 */
class Places(private val app: NotApp) {
    private val prefs = app.getSharedPreferences("places", Context.MODE_PRIVATE)
    private val location = app.getSystemService(LocationManager::class.java)

    fun places(): List<Place> = read("places") { o ->
        Place(o.getString("id"), o.getString("name"), o.getDouble("lat"), o.getDouble("lon"), o.optDouble("radius", 150.0).toFloat())
    }

    fun reminders(): List<PlaceReminder> = read("reminders") { o ->
        PlaceReminder(o.getString("task"), o.getString("place"), o.getBoolean("arriving"))
    }

    fun reminderFor(taskId: String): PlaceReminder? = reminders().firstOrNull { it.taskId == taskId }

    fun addPlace(name: String, at: Location): Place {
        val place = Place(UUID.randomUUID().toString(), name.trim(), at.latitude, at.longitude)
        writePlaces(places() + place)
        return place
    }

    fun deletePlace(id: String) {
        writePlaces(places().filter { it.id != id })
        writeReminders(reminders().filter { it.placeId != id })
        registerAll()
    }

    fun setReminder(taskId: String, placeId: String, arriving: Boolean) {
        writeReminders(reminders().filter { it.taskId != taskId } + PlaceReminder(taskId, placeId, arriving))
        registerAll()
    }

    fun clearReminder(taskId: String) {
        cancel(taskId)
        writeReminders(reminders().filter { it.taskId != taskId })
    }

    val permitted: Boolean
        get() = ContextCompat.checkSelfPermission(app, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED

    /** Alerts only reach a closed app with "Allow all the time". */
    val permittedInBackground: Boolean
        get() = ContextCompat.checkSelfPermission(app, Manifest.permission.ACCESS_BACKGROUND_LOCATION) == PackageManager.PERMISSION_GRANTED

    /**
     * One proximity alert per reminder whose task is still open; reminders of tasks done or deleted
     * elsewhere go. Runs after changes and at boot (alerts don't survive a restart).
     */
    @SuppressLint("MissingPermission")
    fun registerAll() {
        if (!permitted) return
        val places = places().associateBy { it.id }
        val keep = mutableListOf<PlaceReminder>()
        for (r in reminders()) {
            val open = runCatching { app.store.taskById(r.taskId) }.getOrNull()?.completedAt == null
            val place = places[r.placeId]
            if (!open || place == null) {
                cancel(r.taskId)
                continue
            }
            keep += r
            location.addProximityAlert(place.lat, place.lon, place.radius, -1, intent(r.taskId))
        }
        writeReminders(keep)
    }

    /** From a proximity alert: notify when it's the edge the reminder wants, then clear it. */
    internal fun fire(taskId: String, entering: Boolean) {
        val reminder = reminderFor(taskId) ?: return cancel(taskId)
        if (reminder.arriving != entering) return
        val task = runCatching { app.store.taskById(taskId) }.getOrNull()
        clearReminder(taskId)
        if (task == null || task.completedAt != null) return
        val place = places().firstOrNull { it.id == reminder.placeId }?.name ?: "a place"
        app.reminders // makes sure the channels exist
        if (!app.reminders.canNotify) return
        NotificationManagerCompat.from(app).notify(
            taskId.hashCode(),
            NotificationCompat.Builder(app, Reminders.CHANNEL_TASKS)
                .setSmallIcon(R.drawable.ic_place)
                .setContentTitle(task.title)
                .setContentText(if (entering) "You're at $place" else "Leaving $place")
                .setCategory(NotificationCompat.CATEGORY_REMINDER)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(
                    PendingIntent.getActivity(
                        app, 0,
                        Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                    ),
                )
                .addAction(
                    0, "Done",
                    PendingIntent.getBroadcast(
                        app, 0,
                        Intent(app, ReminderReceiver::class.java)
                            .setAction(Reminders.ACTION_DONE)
                            .setData(android.net.Uri.parse("not://place-done/$taskId"))
                            .putExtra(Reminders.EXTRA_ID, taskId),
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                    ),
                )
                .build(),
        )
    }

    /** Where the phone is now (one fresh fix), for saving a place. */
    @SuppressLint("MissingPermission")
    fun currentLocation(onResult: (Location?) -> Unit) {
        if (!permitted) return onResult(null)
        val provider = when {
            Build.VERSION.SDK_INT >= 31 && location.isProviderEnabled(LocationManager.FUSED_PROVIDER) -> LocationManager.FUSED_PROVIDER
            location.isProviderEnabled(LocationManager.GPS_PROVIDER) -> LocationManager.GPS_PROVIDER
            location.isProviderEnabled(LocationManager.NETWORK_PROVIDER) -> LocationManager.NETWORK_PROVIDER
            else -> return onResult(null)
        }
        if (Build.VERSION.SDK_INT >= 30) {
            location.getCurrentLocation(provider, null, ContextCompat.getMainExecutor(app)) { onResult(it ?: location.getLastKnownLocation(provider)) }
        } else {
            onResult(location.getLastKnownLocation(provider))
        }
    }

    private fun cancel(taskId: String) {
        if (permitted) location.removeProximityAlert(intent(taskId))
    }

    private fun intent(taskId: String): PendingIntent =
        PendingIntent.getBroadcast(
            app,
            0,
            Intent(app, PlaceReceiver::class.java).setData(android.net.Uri.parse("not://place/$taskId")).putExtra(EXTRA_TASK, taskId),
            // Mutable: the system adds whether the phone is entering or leaving.
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
        )

    private fun <T> read(key: String, parse: (JSONObject) -> T): List<T> {
        val json = runCatching { JSONArray(prefs.getString(key, "[]")) }.getOrNull() ?: return emptyList()
        return (0 until json.length()).mapNotNull { runCatching { parse(json.getJSONObject(it)) }.getOrNull() }
    }

    private fun writePlaces(places: List<Place>) {
        val json = JSONArray(places.map { JSONObject().put("id", it.id).put("name", it.name).put("lat", it.lat).put("lon", it.lon).put("radius", it.radius.toDouble()) })
        prefs.edit().putString("places", json.toString()).apply()
    }

    private fun writeReminders(reminders: List<PlaceReminder>) {
        val json = JSONArray(reminders.map { JSONObject().put("task", it.taskId).put("place", it.placeId).put("arriving", it.arriving) })
        prefs.edit().putString("reminders", json.toString()).apply()
    }

    companion object {
        internal const val EXTRA_TASK = "task"
    }
}

/** Proximity alerts from [Places]. */
class PlaceReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val task = intent.getStringExtra(Places.EXTRA_TASK) ?: return
        val entering = intent.getBooleanExtra(LocationManager.KEY_PROXIMITY_ENTERING, true)
        (context.applicationContext as NotApp).places.fire(task, entering)
    }
}

/** "At Home" / "Leaving Gym" in a task's sheet; tap to choose a place, or save where you are. */
@Composable
fun PlacePill(task: TaskItem) {
    val context = LocalContext.current
    val places = (context.applicationContext as NotApp).places
    var version by remember { mutableIntStateOf(0) }
    var editing by remember { mutableStateOf(false) }
    val reminder = remember(task.id, version) { places.reminderFor(task.id) }
    val place = remember(reminder, version) { reminder?.let { r -> places.places().firstOrNull { it.id == r.placeId } } }
    Chip(
        R.drawable.ic_place,
        when {
            place == null -> "Place"
            reminder!!.arriving -> "At ${place.name}"
            else -> "Leaving ${place.name}"
        },
        tint = if (place == null) Not.colors.inkMuted else null,
        onClick = { editing = true },
    )
    if (editing) {
        PlaceDialog(task, places, onChanged = { version++ }, onDismiss = { editing = false })
    }
}

@Composable
private fun PlaceDialog(task: TaskItem, places: Places, onChanged: () -> Unit, onDismiss: () -> Unit) {
    var version by remember { mutableIntStateOf(0) }
    val saved = remember(version) { places.places() }
    val current = remember(version) { places.reminderFor(task.id) }
    var arriving by remember { mutableStateOf(current?.arriving ?: true) }
    var naming by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("") }
    var status by remember { mutableStateOf<String?>(null) }
    val background = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { places.registerAll() }
    val foreground = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        version++
        // Android asks for "all the time" on its own screen, after the first grant.
        if (places.permitted && !places.permittedInBackground) background.launch(Manifest.permission.ACCESS_BACKGROUND_LOCATION)
    }
    fun pick(place: Place) {
        places.setReminder(task.id, place.id, arriving)
        onChanged()
        onDismiss()
    }
    CardDialog(
        title = "Remind at a place",
        onDismiss = onDismiss,
        buttons = {
            if (current != null) Button("Remove", {
                places.clearReminder(task.id)
                onChanged()
                onDismiss()
            }, kind = ButtonKind.Quiet)
            Button("Close", onDismiss, kind = ButtonKind.Quiet)
        },
    ) {
        if (!places.permitted) {
            Text("Reminders at places need location access. They stay on this phone.", color = Not.colors.inkMuted)
            Button("Allow location", {
                foreground.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
            }, modifier = Modifier.padding(top = 12.dp))
            return@CardDialog
        }
        if (!places.permittedInBackground) {
            Text("Choose “Allow all the time” so reminders arrive while the app is closed.", style = Not.type.caption, color = Not.colors.danger)
            Button("Allow all the time", { background.launch(Manifest.permission.ACCESS_BACKGROUND_LOCATION) }, kind = ButtonKind.Secondary, modifier = Modifier.padding(vertical = 8.dp))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(bottom = 8.dp)) {
            Chip(R.drawable.ic_place, "Arriving", tint = if (arriving) null else Not.colors.inkMuted, background = if (arriving) null else Not.colors.sunken, onClick = { arriving = true })
            Chip(R.drawable.ic_place, "Leaving", tint = if (!arriving) null else Not.colors.inkMuted, background = if (!arriving) null else Not.colors.sunken, onClick = { arriving = false })
        }
        saved.forEach { place ->
            ListRow(R.drawable.ic_place, place.name, selected = current?.placeId == place.id, onClick = { pick(place) })
        }
        if (naming) {
            Field(name, { name = it }, "Name this place: Home, Gym…", filled = true, modifier = Modifier.fillMaxWidth().padding(top = 8.dp))
            status?.let { Text(it, style = Not.type.caption, color = Not.colors.inkMuted, modifier = Modifier.padding(top = 4.dp)) }
            Button("Save where I am", {
                status = "Finding you…"
                places.currentLocation { at ->
                    if (at == null) {
                        status = "Couldn't get a location. Is location turned on?"
                    } else {
                        pick(places.addPlace(name.ifBlank { "Place ${saved.size + 1}" }, at))
                    }
                }
            }, enabled = name.isNotBlank(), modifier = Modifier.padding(top = 8.dp))
        } else {
            ListRow(R.drawable.ic_add, "Save where I am…", accent = true, onClick = { naming = true })
        }
    }
}
