package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.view.PixelCopy
import android.view.View
import android.view.WindowManager
import android.view.inspector.WindowInspector
import androidx.annotation.RequiresApi
import kotlinx.coroutines.suspendCancellableCoroutine
import java.util.concurrent.Executor
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** Reconstructs the app image because Android reports screenshots without providing their bitmap. */
@RequiresApi(34)
internal suspend fun captureAppScreenshot(activity: Activity): Bitmap {
    val root = activity.window.decorView
    val parameters = root.layoutParams as WindowManager.LayoutParams
    val visible = WindowInspector.getGlobalWindowViews().filter { it.isShown }
    val windows = visible.filter { (it.layoutParams as WindowManager.LayoutParams).token == parameters.token }
    val tokens = windows.map { it.windowToken }.toSet()
    val overlays = visible.filter { it !== root && (it in windows || it.applicationWindowToken in tokens) }
    val bitmap = copyWindow(root, activity.mainExecutor)
    val canvas = Canvas(bitmap)
    val origin = IntArray(2).also(root::getLocationOnScreen)

    for (overlay in overlays) {
        val image = copyWindow(overlay, activity.mainExecutor)
        val position = IntArray(2).also(overlay::getLocationOnScreen)
        val attributes = overlay.layoutParams as WindowManager.LayoutParams
        if (attributes.flags and WindowManager.LayoutParams.FLAG_DIM_BEHIND != 0) {
            canvas.drawARGB((attributes.dimAmount * 255).toInt(), 0, 0, 0)
        }
        canvas.drawBitmap(image, (position[0] - origin[0]).toFloat(), (position[1] - origin[1]).toFloat(), null)
    }
    return bitmap
}

@RequiresApi(34)
private suspend fun copyWindow(view: View, executor: Executor): Bitmap = suspendCancellableCoroutine { continuation ->
    val request = PixelCopy.Request.Builder.ofWindow(view)
        .setSourceRect(Rect(0, 0, view.width, view.height))
        .build()
    PixelCopy.request(request, executor) { result ->
        if (continuation.isActive) {
            if (result.status == PixelCopy.SUCCESS) continuation.resume(result.bitmap)
            else continuation.resumeWithException(IllegalStateException("Screenshot capture failed: ${result.status}"))
        }
    }
}
