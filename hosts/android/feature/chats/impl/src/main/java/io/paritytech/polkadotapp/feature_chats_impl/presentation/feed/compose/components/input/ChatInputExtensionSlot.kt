package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.clearAndSetSemantics
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatInputSlotRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.LocalChatInputSlotReady
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private const val SLOT_TRANSITION_MILLIS = 250

// A renderer that never reports its content still takes the slot after this long.
private const val SLOT_READY_TIMEOUT_MILLIS = 3_000L

@Stable
private class SlotLayer(val id: Int, renderer: CustomChatInputSlotRenderer, isShown: Boolean) {
    var renderer by mutableStateOf(renderer)
    var isReady by mutableStateOf(isShown || !renderer.reportsReady)
    var height by mutableIntStateOf(-1)
    val alpha = Animatable(if (isShown) 1f else 0f)
    val onReady: () -> Unit = { isReady = true }
}

/**
 * The input slot an extension fills.
 *
 * When another kind of renderer takes the slot, the one before stays on screen until the new one has drawn its content;
 * then they crossfade and the slot's height follows, so the slot is never empty and the chat above it moves instead of
 * jumping. A renderer of the same kind takes over in place. The slot does not clip: a renderer may draw above it.
 */
@Composable
internal fun ChatInputExtensionSlot(
    renderer: CustomChatInputSlotRenderer,
    modifier: Modifier = Modifier,
) {
    var nextId by remember { mutableIntStateOf(1) }
    val layers = remember { mutableStateListOf(SlotLayer(0, renderer, isShown = true)) }

    LaunchedEffect(renderer) {
        val newest = layers.last()
        when {
            newest.renderer == renderer -> Unit
            newest.renderer::class == renderer::class -> newest.renderer = renderer
            else -> layers += SlotLayer(nextId++, renderer, isShown = false)
        }
    }

    // The newest layer with content is the one shown; the layers before it fade out and leave.
    val front = layers.lastOrNull { it.isReady } ?: layers.first()
    LaunchedEffect(front) {
        val leaving = layers.takeWhile { it !== front }
        coroutineScope {
            launch { front.alpha.animateTo(1f, tween(SLOT_TRANSITION_MILLIS, easing = FastOutSlowInEasing)) }
            leaving.forEach { layer -> launch { layer.alpha.animateTo(0f, tween(SLOT_TRANSITION_MILLIS, easing = FastOutSlowInEasing)) } }
        }
        layers.removeAll(leaving)
    }

    val height = remember { Animatable(0f) }
    var isHeightSet by remember { mutableStateOf(false) }
    val frontHeight = front.height
    LaunchedEffect(frontHeight) {
        if (frontHeight < 0) return@LaunchedEffect
        if (isHeightSet) {
            height.animateTo(frontHeight.toFloat(), tween(SLOT_TRANSITION_MILLIS, easing = FastOutSlowInEasing))
        } else {
            height.snapTo(frontHeight.toFloat())
            isHeightSet = true
        }
    }

    Layout(
        modifier = modifier,
        content = {
            layers.forEach { layer ->
                key(layer.id) {
                    SlotLayerContent(layer, isFront = layer === front)
                }
            }
        },
    ) { measurables, constraints ->
        val childConstraints = constraints.copy(minHeight = 0)
        val placeables = measurables.map { it.measure(childConstraints) }
        val frontIndex = layers.indexOf(front)
        val naturalHeight = placeables.getOrNull(frontIndex)?.height ?: 0
        val slotHeight = (if (isHeightSet) height.value.roundToInt() else naturalHeight)
            .coerceIn(constraints.minHeight, constraints.maxHeight)
        val slotWidth = placeables.maxOfOrNull { it.width }?.coerceIn(constraints.minWidth, constraints.maxWidth) ?: constraints.minWidth
        layout(slotWidth, slotHeight) {
            // Every layer sits on the slot's bottom edge; the shown one is on top, and takes the touches.
            placeables.forEachIndexed { index, placeable ->
                placeable.place(0, slotHeight - placeable.height, zIndex = if (index == frontIndex) 1f else 0f)
            }
        }
    }
}

@Composable
private fun SlotLayerContent(layer: SlotLayer, isFront: Boolean) {
    if (!layer.isReady) {
        LaunchedEffect(Unit) {
            delay(SLOT_READY_TIMEOUT_MILLIS)
            layer.isReady = true
        }
    }
    Box(
        modifier = Modifier
            // Alpha per draw, without an offscreen layer.
            .graphicsLayer {
                alpha = layer.alpha.value
                compositingStrategy = CompositingStrategy.ModulateAlpha
            }
            .onSizeChanged { layer.height = it.height }
            .then(if (isFront) Modifier else Modifier.clearAndSetSemantics {})
    ) {
        CompositionLocalProvider(LocalChatInputSlotReady provides layer.onReady) {
            layer.renderer.drawInputSlot()
        }
        if (!isFront) {
            // A layer that is still loading or already leaving takes no touches.
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .pointerInput(Unit) {
                        awaitPointerEventScope {
                            while (true) {
                                awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
                            }
                        }
                    }
            )
        }
    }
}
