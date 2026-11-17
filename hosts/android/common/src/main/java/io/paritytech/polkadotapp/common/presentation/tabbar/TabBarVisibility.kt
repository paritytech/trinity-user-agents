package io.paritytech.polkadotapp.common.presentation.tabbar

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
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
 * Which [TabBarMode] the bar is in. It exists only on screens that pin it via [ForceShowTabBar] (Main) or
 * [ShowTabBarTucked], and only while no screen pins [HideTabBar]; everywhere else it is absent entirely.
 * There is deliberately no opt-out state in between, so a screen cannot end up with a bar it did not ask for.
 */
@Singleton
class TabBarVisibilityHolder @Inject constructor() {
    private val pins = LinkedHashMap<Any, TabBarPin>()

    val mode: StateFlow<TabBarMode>
        field = MutableStateFlow<TabBarMode>(TabBarMode.Absent)

    @Synchronized
    fun pin(key: Any, pin: TabBarPin) {
        pins[key] = pin
        recompute()
    }

    @Synchronized
    fun release(key: Any) {
        pins.remove(key)
        recompute()
    }

    private fun recompute() {
        val hiding = pins.lastKeyPinning(TabBarPin.Hidden)
        val tucking = pins.lastKeyPinning(TabBarPin.Tucked)
        mode.value = when {
            hiding != null -> TabBarMode.HiddenByScreen(hiding)
            tucking != null -> TabBarMode.TuckedOverScreen(tucking)
            pins.isNotEmpty() -> TabBarMode.Main
            else -> TabBarMode.Absent
        }
    }

    private fun Map<Any, TabBarPin>.lastKeyPinning(pin: TabBarPin): Any? = entries.lastOrNull { it.value == pin }?.key
}

enum class TabBarPin { Main, Tucked, Hidden }

@Immutable
sealed interface TabBarMode {
    val showsBar: Boolean
        get() = this == Main || this is TuckedOverScreen

    data object Absent : TabBarMode

    data object Main : TabBarMode

    data class TuckedOverScreen(val screen: Any) : TabBarMode

    data class HiddenByScreen(val screen: Any) : TabBarMode
}

val LocalTabBarVisibility = staticCompositionLocalOf<TabBarVisibilityHolder?> { null }

/**
 * Pins the bar from the host lifecycle's first resume until the composition leaves; outside taps pass through
 * to content (used on Main). Screens that never call this have no bar.
 */
@Composable
fun ForceShowTabBar() {
    PinTabBar(TabBarPin.Main)
}

@Composable
fun ShowTabBarTucked() {
    PinTabBar(TabBarPin.Tucked)
}

@Composable
fun HideTabBar() {
    PinTabBar(TabBarPin.Hidden)
}

@Composable
private fun PinTabBar(pin: TabBarPin) {
    val holder = LocalTabBarVisibility.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    // A back-gesture preview composes the screen unresumed; releasing on pause would republish a zero bar height.
    val resumed by produceState(false, lifecycle) { lifecycle.withResumed { value = true } }

    if (holder != null && resumed) {
        DisposableEffect(holder) {
            val key = Any()
            holder.pin(key, pin)
            onDispose { holder.release(key) }
        }
    }
}
