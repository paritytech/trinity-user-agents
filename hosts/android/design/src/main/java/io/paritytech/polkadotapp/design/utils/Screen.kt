package io.paritytech.polkadotapp.design.utils

import android.content.pm.ActivityInfo
import androidx.activity.compose.LocalActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalView

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
