package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.compose

import android.webkit.WebView
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.BrowserChromeCollapseUpdate
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.BrowserChromeCollapser
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.BrowserChromeScrollSample
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.onStart
import kotlin.math.roundToInt

@Stable
internal class BrowserChromeCollapse {
    private val animatedFraction = Animatable(0f)
    private val isTouching = MutableStateFlow(false)

    val fraction: Float get() = animatedFraction.value

    val touchTracking: Modifier = Modifier.pointerInput(Unit) {
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            isTouching.value = true
            do {
                val event = awaitPointerEvent(PointerEventPass.Initial)
            } while (event.changes.any { it.pressed })
            isTouching.value = false
        }
    }

    suspend fun follow(webView: WebView, collapser: BrowserChromeCollapser) {
        animatedFraction.snapTo(0f)

        val scrollOffsets = callbackFlow {
            webView.setOnScrollChangeListener { _, _, scrollY, _, _ -> trySend(scrollY) }
            awaitClose { webView.setOnScrollChangeListener(null) }
        }.onStart { emit(webView.scrollY) }

        combine(scrollOffsets, isTouching, ::Pair).collectLatest { (scrollY, touching) ->
            apply(collapser.update(BrowserChromeScrollSample(scrollY.toFloat(), isInteracting = true)))

            // A fling keeps scrolling after the finger lifts, so the scroll is at rest only once it stops moving.
            if (!touching) {
                delay(SCROLL_REST_MILLIS)
                apply(collapser.update(BrowserChromeScrollSample(scrollY.toFloat(), isInteracting = false)))
            }
        }
    }

    private suspend fun apply(update: BrowserChromeCollapseUpdate?) {
        when {
            update == null -> Unit
            update.animated -> animatedFraction.animateTo(update.fraction, tween(SETTLE_MILLIS))
            else -> animatedFraction.snapTo(update.fraction)
        }
    }
}

@Composable
internal fun rememberBrowserChromeCollapse(webView: WebView?, topPanelHeight: Int): BrowserChromeCollapse {
    val collapse = remember { BrowserChromeCollapse() }

    LaunchedEffect(webView, topPanelHeight) {
        webView?.let { collapse.follow(it, BrowserChromeCollapser(topPanelHeight.toFloat())) }
    }

    return collapse
}

// A fully collapsed panel is not placed at all, so it is neither tappable nor read out while hidden.
internal fun Modifier.collapsedBy(fraction: () -> Float): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints)
    val shown = 1f - fraction()

    layout(placeable.width, (placeable.height * shown).roundToInt()) {
        if (shown > 0f) {
            placeable.placeWithLayer(0, 0) {
                scaleX = shown
                scaleY = shown
                alpha = shown
            }
        }
    }
}

private const val SCROLL_REST_MILLIS = 150L
private const val SETTLE_MILLIS = 250
