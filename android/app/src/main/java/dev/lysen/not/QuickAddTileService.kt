package dev.lysen.not

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/** Quick Settings tile that opens the quick-add dialog for the Inbox. */
class QuickAddTileService : TileService() {
    override fun onStartListening() {
        qsTile?.apply {
            state = Tile.STATE_INACTIVE
            subtitle = getString(R.string.add_task_subtitle)
            updateTile()
        }
    }

    override fun onClick() {
        // The dialog must not appear on top of the lock screen.
        if (isLocked) unlockAndRun(::openQuickAdd) else openQuickAdd()
    }

    // The Intent overload is only called below API 34, where it is the only one.
    @SuppressLint("StartActivityAndCollapseDeprecated")
    private fun openQuickAdd() {
        val intent = QuickAddActivity.intent(this, INBOX_ID).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
