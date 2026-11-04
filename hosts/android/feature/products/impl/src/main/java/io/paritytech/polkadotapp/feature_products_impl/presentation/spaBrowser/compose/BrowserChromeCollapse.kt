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
import androidx.compose.ui.graphics.TransformOrigin
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
    private var targetFraction = 0f

    val fraction: Float get() = animatedFraction.value

    val touchTracking: Modifier = Modifier.pointerInput(Unit) {
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            isTouching.value = true
            try {
                do {
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                } while (event.changes.any { it.pressed })
            } finally {
                isTouching.value = false
            }
        }
    }

    suspend fun follow(webView: WebView, collapser: BrowserChromeCollapser) {
        targetFraction = 0f
        animatedFraction.snapTo(0f)

        val scrollOffsets = callbackFlow {
            webView.setOnScrollChangeListener { _, _, scrollY, _, _ -> trySend(scrollY) }
            awaitClose { webView.setOnScrollChangeListener(null) }
        }.onStart { emit(webView.scrollY) }

        combine(scrollOffsets, isTouching, ::Pair).collectLatest { (scrollY, touching) ->
            applyUpdate(collapser.update(BrowserChromeScrollSample(scrollY.toFloat(), isInteracting = true)))

            // A fling keeps scrolling after the finger lifts, so the scroll is at rest only once it stops moving.
            if (!touching) {
                delay(SCROLL_REST_MILLIS)
                applyUpdate(collapser.update(BrowserChromeScrollSample(scrollY.toFloat(), isInteracting = false)))
                // A new touch can cancel a settle midway, and the collapser reports no change for the end it already chose.
                if (animatedFraction.value != targetFraction) animatedFraction.animateTo(targetFraction, tween(SETTLE_MILLIS))
            }
        }
    }

    private suspend fun applyUpdate(update: BrowserChromeCollapseUpdate?) {
        if (update == null) return
        targetFraction = update.fraction
        if (update.animated) {
            animatedFraction.animateTo(update.fraction, tween(SETTLE_MILLIS))
        } else {
            animatedFraction.snapTo(update.fraction)
        }
    }
}

@Composable
internal fun rememberBrowserChromeCollapse(webView: WebView?, topPanelHeight: Int, reloadCount: Int): BrowserChromeCollapse {
    val collapse = remember { BrowserChromeCollapse() }

    LaunchedEffect(webView, topPanelHeight, reloadCount) {
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
                transformOrigin = TransformOrigin(0.5f, 0f)
            }
        }
    }
}

private const val SCROLL_REST_MILLIS = 150L
private const val SETTLE_MILLIS = 250
