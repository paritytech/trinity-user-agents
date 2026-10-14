package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue

import android.app.Activity
import android.graphics.Bitmap
import androidx.annotation.RequiresApi
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.navigation.NavController
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import io.paritytech.polkadotapp.common.R as RCommon

@RequiresApi(34)
class DebugScreenshotObserver(
    private val activity: AppCompatActivity,
    private val navController: NavController,
    private val router: RootRouter,
    private val notifier: AppNotifier,
) : DefaultLifecycleObserver {
    private var captureJob: Job? = null
    private val callback = Activity.ScreenCaptureCallback { reportScreenshot() }

    override fun onStart(owner: LifecycleOwner) {
        activity.registerScreenCaptureCallback(activity.mainExecutor, callback)
    }

    override fun onStop(owner: LifecycleOwner) {
        activity.unregisterScreenCaptureCallback(callback)
        captureJob?.cancel()
    }

    private fun reportScreenshot() {
        if (captureJob?.isActive == true || navController.currentDestination?.id == R.id.reportIssueBottomSheet) return

        captureJob = activity.lifecycleScope.launch {
            var screenshot: File? = null
            var presented = false
            try {
                runCancellableCatching {
                    val bitmap = captureAppScreenshot(activity)
                    withContext(Dispatchers.IO) {
                        val file = File.createTempFile("issue-screenshot-", ".png", activity.cacheDir)
                        screenshot = file
                        file.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
                    }
                    router.openIssueReport(requireNotNull(screenshot).path)
                    presented = true
                }.logFailure("Capture issue screenshot")
                    .onFailure { notifier.error(activity.getString(RCommon.string.debug_report_capture_failed)) }
            } finally {
                if (!presented) {
                    withContext(NonCancellable + Dispatchers.IO) { screenshot?.delete() }
                }
            }
        }
    }
}
