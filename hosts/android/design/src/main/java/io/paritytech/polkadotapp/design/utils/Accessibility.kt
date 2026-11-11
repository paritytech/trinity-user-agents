package io.paritytech.polkadotapp.design.utils

import android.view.accessibility.AccessibilityManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext

@Composable
fun rememberTouchExplorationEnabled(): Boolean {
    val context = LocalContext.current
    var enabled by remember { mutableStateOf(false) }
    DisposableEffect(context) {
        val manager = context.getSystemService(AccessibilityManager::class.java)
        enabled = manager.isTouchExplorationEnabled
        val listener = AccessibilityManager.TouchExplorationStateChangeListener { enabled = it }
        manager.addTouchExplorationStateChangeListener(listener)
        onDispose { manager.removeTouchExplorationStateChangeListener(listener) }
    }
    return enabled
}
