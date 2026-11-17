package dev.lysen.not.screens

import androidx.work.testing.WorkManagerTestInitHelper
import dev.lysen.not.NotApp

/** The app as screen tests run it: WorkManager's test version, since Robolectric sets up none. */
class TestApp : NotApp() {
    override fun onCreate() {
        WorkManagerTestInitHelper.initializeTestWorkManager(this)
        super.onCreate()
    }
}
