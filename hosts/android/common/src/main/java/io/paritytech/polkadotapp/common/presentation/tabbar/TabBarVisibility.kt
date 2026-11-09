package io.paritytech.polkadotapp.common.presentation.tabbar

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.withResumed
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Whether the bar is shown at all. It exists only on screens that pin it via [ForceShowTabBar] (Main) or
 * [ShowTabBarTucked]; everywhere else it is absent entirely: no nub, nothing to pull out.
 * There is deliberately no opt-out state in between, so a screen cannot end up with a bar it did not ask for.
 */
@Singleton
class TabBarVisibilityHolder @Inject constructor() {
    private val forceKeys = mutableSetOf<Any>()

    val hidden: StateFlow<Boolean>
        field = MutableStateFlow(true)

    // Whether some screen forces the bar shown (e.g. Main) — its outside-tap scrim is disabled so content
    // stays clickable.
    val forced: StateFlow<Boolean>
        field = MutableStateFlow(false)

    val tuckedPin: StateFlow<TabBarTuckedPin?>
        field = MutableStateFlow(null)

    val hiddenPin: StateFlow<TabBarHiddenPin?>
        field = MutableStateFlow(null)

    @Synchronized
    fun pin(key: Any) {
        forceKeys.add(key)
        recompute()
    }

    @Synchronized
    fun release(key: Any) {
        forceKeys.remove(key)
        recompute()
    }

    private fun recompute() {
        val showing = forceKeys.any { it !is TabBarHiddenPin }
        forced.value = showing
        hidden.value = !showing
        tuckedPin.value = forceKeys.filterIsInstance<TabBarTuckedPin>().lastOrNull()
        hiddenPin.value = forceKeys.filterIsInstance<TabBarHiddenPin>().lastOrNull()
    }
}

class TabBarTuckedPin

class TabBarHiddenPin

val LocalTabBarVisibility = staticCompositionLocalOf<TabBarVisibilityHolder?> { null }

/**
 * Pins the bar from the host lifecycle's first resume until the composition leaves; outside taps pass through
 * to content (used on Main). Screens that never call this have no bar.
 */
@Composable
fun ForceShowTabBar() {
    PinTabBar(::Any)
}

@Composable
fun ShowTabBarTucked() {
    PinTabBar(::TabBarTuckedPin)
}

@Composable
fun HideTabBar() {
    PinTabBar(::TabBarHiddenPin)
}

@Composable
private fun PinTabBar(newKey: () -> Any) {
    val holder = LocalTabBarVisibility.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    // A back-gesture preview composes the screen unresumed; releasing on pause would republish a zero bar height.
    val resumed by produceState(false, lifecycle) { lifecycle.withResumed { value = true } }

    if (holder != null && resumed) {
        DisposableEffect(holder) {
            val key = newKey()
            holder.pin(key)
            onDispose { holder.release(key) }
        }
    }
}
