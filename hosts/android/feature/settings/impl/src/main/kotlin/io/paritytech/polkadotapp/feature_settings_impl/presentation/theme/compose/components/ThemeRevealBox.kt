package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot

private const val RevealDurationMillis = 500

/**
 * Holds a pending circular-reveal request raised from the UI. [reveal] records where the
 * theme change was triggered (in root coordinates) and the action that actually applies it,
 * which runs only after the old appearance has been snapshotted.
 */
@Stable
internal class ThemeRevealState {
    internal var request by mutableStateOf<ThemeRevealRequest?>(null)
        private set

    fun reveal(centerInRoot: Offset, applyTheme: () -> Unit) {
        request = ThemeRevealRequest(centerInRoot, applyTheme)
    }

    internal fun consume(request: ThemeRevealRequest) {
        if (this.request === request) this.request = null
    }
}

internal class ThemeRevealRequest(
    val centerInRoot: Offset,
    val applyTheme: () -> Unit
)

@Composable
internal fun rememberThemeRevealState(): ThemeRevealState = remember { ThemeRevealState() }

/**
 * Wraps [content] and plays a circular reveal whenever [state] receives a request: the current
 * appearance is captured into a snapshot, the new theme is applied underneath, and the snapshot
 * is wiped away through an expanding hole centered on the tapped theme.
 */
@Composable
internal fun ThemeRevealBox(
    state: ThemeRevealState,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit
) {
    val graphicsLayer = rememberGraphicsLayer()
    val radius = remember { Animatable(0f) }

    var overlay by remember { mutableStateOf<ImageBitmap?>(null) }
    var center by remember { mutableStateOf(Offset.Zero) }
    var positionInRoot by remember { mutableStateOf(Offset.Zero) }

    Box(
        modifier = modifier
            .onGloballyPositioned { positionInRoot = it.positionInRoot() }
            .drawWithContent {
                graphicsLayer.record { this@drawWithContent.drawContent() }
                drawLayer(graphicsLayer)

                val snapshot = overlay ?: return@drawWithContent
                val canvas = drawContext.canvas
                canvas.saveLayer(Rect(Offset.Zero, size), Paint())
                drawImage(snapshot)
                drawCircle(
                    color = Color.Black,
                    radius = radius.value,
                    center = center,
                    blendMode = BlendMode.Clear
                )
                canvas.restore()
            }
    ) {
        content()
    }

    val request = state.request
    LaunchedEffect(request) {
        if (request == null) return@LaunchedEffect

        val localCenter = request.centerInRoot - positionInRoot
        val width = graphicsLayer.size.width.toFloat()
        val height = graphicsLayer.size.height.toFloat()
        val targetRadius = listOf(
            Offset(0f, 0f),
            Offset(width, 0f),
            Offset(0f, height),
            Offset(width, height)
        ).maxOf { (it - localCenter).getDistance() }

        center = localCenter
        overlay = graphicsLayer.toImageBitmap()
        radius.snapTo(0f)

        request.applyTheme()

        radius.animateTo(
            targetValue = targetRadius,
            animationSpec = tween(durationMillis = RevealDurationMillis, easing = FastOutSlowInEasing)
        )

        overlay = null
        state.consume(request)
    }
}
