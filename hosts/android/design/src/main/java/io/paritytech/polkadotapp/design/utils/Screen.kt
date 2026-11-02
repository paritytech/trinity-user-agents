package io.paritytech.polkadotapp.design.utils

import android.content.pm.ActivityInfo
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.view.accessibility.AccessibilityEvent
import androidx.activity.compose.LocalActivity
import androidx.annotation.RequiresApi
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.platform.LocalView
import io.paritytech.polkadotapp.design.BuildConfig
import io.paritytech.polkadotapp.design.R
import timber.log.Timber

@Composable
fun KeepScreenOn() {
    val currentView = LocalView.current

    DisposableEffect(Unit) {
        val originalState = currentView.keepScreenOn

        currentView.keepScreenOn = true

        onDispose {
            currentView.keepScreenOn = originalState
        }
    }
}

@Composable
fun LockScreenOrientation() {
    LocalActivity.current?.let { activity ->
        DisposableEffect(Unit) {
            val originalOrientation = activity.requestedOrientation

            activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LOCKED

            onDispose {
                activity.requestedOrientation = originalOrientation
            }
        }
    }
}

private const val UNPROTECTED_SCREEN_MESSAGE = "LocalView parent is not a ViewGroup, the screen is not protected"

@Composable
fun SensitiveScreenEffect() {
    // AndroidComposeView never runs onFilterTouchEventForSecurity, so the flag goes on the view above it.
    val hostView = LocalView.current.parent as? ViewGroup

    if (hostView == null) {
        LaunchedEffect(Unit) {
            if (BuildConfig.DEBUG) error(UNPROTECTED_SCREEN_MESSAGE) else Timber.e(UNPROTECTED_SCREEN_MESSAGE)
        }
    } else {
        DisposableEffect(hostView) {
            val originalFilter = hostView.filterTouchesWhenObscured
            val originalDelegate = hostView.accessibilityDelegate

            hostView.filterTouchesWhenObscured = true
            hostView.accessibilityDelegate = SensitiveEventsAccessibilityDelegate

            onDispose {
                hostView.filterTouchesWhenObscured = originalFilter
                hostView.accessibilityDelegate = originalDelegate
            }
        }
    }

    val window = LocalActivity.current?.window

    if (window != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        DisposableEffect(window) {
            window.retainOverlayHiding()

            onDispose {
                window.releaseOverlayHiding()
            }
        }
    }
}

// Fragments share the activity window, so restoring a saved value would clear it under the next screen.
@RequiresApi(Build.VERSION_CODES.S)
internal fun Window.retainOverlayHiding() {
    val count = decorView.getTag(R.id.overlay_hiding_count) as? Int ?: 0
    if (count == 0) {
        setHideOverlayWindows(true)
    }
    decorView.setTag(R.id.overlay_hiding_count, count + 1)
}

@RequiresApi(Build.VERSION_CODES.S)
internal fun Window.releaseOverlayHiding() {
    val count = decorView.getTag(R.id.overlay_hiding_count) as Int - 1
    if (count == 0) {
        setHideOverlayWindows(false)
    }
    decorView.setTag(R.id.overlay_hiding_count, count)
}

private object SensitiveEventsAccessibilityDelegate : View.AccessibilityDelegate() {
    override fun onRequestSendAccessibilityEvent(host: ViewGroup, child: View, event: AccessibilityEvent): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            event.isAccessibilityDataSensitive = true
        }

        return super.onRequestSendAccessibilityEvent(host, child, event)
    }
}
