package dev.lysen.not

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dev.lysen.not.core.NotException
import java.util.concurrent.TimeUnit

/**
 * One sync round in the background. Live sync only runs while the process is awake, so this keeps
 * widgets fresh (every 15 minutes) and gets edits made from widgets and quick add to the server
 * even if the app is never opened.
 */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result =
        try {
            (applicationContext as NotApp).store.sync()
            Result.success()
        } catch (_: NotException) {
            if (runAttemptCount < MAX_ATTEMPTS) Result.retry() else Result.failure()
        }

    companion object {
        private const val MAX_ATTEMPTS = 5
        private val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

        fun schedulePeriodic(context: Context) {
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                "periodic-sync",
                ExistingPeriodicWorkPolicy.KEEP,
                PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES).setConstraints(online).build(),
            )
        }

        /** Pushes local edits once there is network, even if the process is killed first. */
        fun pushSoon(context: Context) {
            WorkManager.getInstance(context).enqueueUniqueWork(
                "push",
                ExistingWorkPolicy.REPLACE,
                OneTimeWorkRequestBuilder<SyncWorker>().setConstraints(online).build(),
            )
        }
    }
}
