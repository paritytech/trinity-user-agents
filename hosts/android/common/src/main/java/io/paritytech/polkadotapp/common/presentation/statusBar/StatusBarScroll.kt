package io.paritytech.polkadotapp.common.presentation.statusBar

import android.webkit.WebView
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

private const val HIDE_AFTER_OFFSET_DP = 40f
private const val HIDE_AFTER_SCROLL_DP = 16f
private const val SHOW_AFTER_SCROLL_DP = 10f

@Immutable
data class StatusBarScrollState(
    val idBarHidden: Boolean,
    val contentUnderBar: Boolean,
) {
    companion object {
        val AtRest = StatusBarScrollState(idBarHidden = false, contentUnderBar = false)
    }
}

@Singleton
class StatusBarScrollHolder @Inject constructor() {
    val state: StateFlow<StatusBarScrollState>
        field = MutableStateFlow(StatusBarScrollState.AtRest)

    private var offsetDp = 0f
    private var runDp = 0f

    @Synchronized
    fun onScroll(downwardDp: Float, atTop: Boolean) {
        if (atTop) {
            reset()
        } else if (downwardDp != 0f) {
            offsetDp = (offsetDp + downwardDp).coerceAtLeast(0f)
            runDp = if (runDp * downwardDp > 0f) runDp + downwardDp else downwardDp

            val hidden = state.value.idBarHidden
            state.value = StatusBarScrollState(
                idBarHidden = when {
                    !hidden -> offsetDp > HIDE_AFTER_OFFSET_DP && runDp >= HIDE_AFTER_SCROLL_DP
                    else -> -runDp < SHOW_AFTER_SCROLL_DP
                },
                contentUnderBar = true,
            )
        }
    }

    @Synchronized
    fun reset() {
        offsetDp = 0f
        runDp = 0f
        state.value = StatusBarScrollState.AtRest
    }
}

val LocalStatusBarScroll = staticCompositionLocalOf<StatusBarScrollHolder?> { null }

@Composable
fun Modifier.reportsScrollToStatusBar(state: ScrollableState): Modifier {
    val holder = LocalStatusBarScroll.current
    val density = LocalDensity.current
    val connection = remember(holder, state, density) {
        object : NestedScrollConnection {
            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (consumed.y != 0f) {
                    holder?.onScroll(downwardDp = with(density) { -consumed.y.toDp().value }, atTop = !state.canScrollBackward)
                }
                return Offset.Zero
            }
        }
    }

    DisposableEffect(holder, state) {
        onDispose { holder?.reset() }
    }

    return if (holder != null) nestedScroll(connection) else this
}

@Composable
fun ReportsWebViewScrollToStatusBar(webView: WebView?) {
    val holder = LocalStatusBarScroll.current
    val density = LocalDensity.current.density

    DisposableEffect(holder, webView) {
        holder?.reset()
        webView?.setOnScrollChangeListener { _, _, scrollY, _, oldScrollY ->
            holder?.onScroll(downwardDp = (scrollY - oldScrollY) / density, atTop = scrollY == 0)
        }
        onDispose {
            webView?.setOnScrollChangeListener(null)
            holder?.reset()
        }
    }
}
