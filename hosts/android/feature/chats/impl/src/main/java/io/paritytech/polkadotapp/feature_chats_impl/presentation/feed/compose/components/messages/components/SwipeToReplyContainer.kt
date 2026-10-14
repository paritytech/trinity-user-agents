package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ReplyArrow
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.launch
import kotlin.math.abs

@Composable
internal fun SwipeToReplyContainer(
    enabled: Boolean = true,
    onReply: () -> Unit,
    content: @Composable BoxScope.() -> Unit,
) {
    val density = LocalDensity.current
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()

    val swipeThresholdPx = with(density) { 44.dp.toPx() }
    val maxOffsetPx = with(density) { 66.dp.toPx() }

    val offsetX = remember { Animatable(0f) }
    var hasTriggeredHaptic by remember { mutableStateOf(false) }

    val draggableState = rememberDraggableState { delta ->
        val newOffset = (offsetX.value + delta).coerceIn(-maxOffsetPx, 0f)

        scope.launch { offsetX.snapTo(newOffset) }

        val passedThreshold = newOffset <= -swipeThresholdPx

        if (passedThreshold && !hasTriggeredHaptic) {
            hasTriggeredHaptic = true
            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
        } else if (!passedThreshold && hasTriggeredHaptic) {
            hasTriggeredHaptic = false
        }
    }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .draggable(
                orientation = Orientation.Horizontal,
                state = draggableState,
                enabled = enabled,
                onDragStopped = {
                    val distance = offsetX.value
                    val shouldReply = distance <= -swipeThresholdPx

                    if (shouldReply) {
                        onReply()
                    }

                    hasTriggeredHaptic = false

                    scope.launch {
                        offsetX.animateTo(
                            targetValue = 0f,
                            animationSpec = tween(durationMillis = 180),
                        )
                    }
                }
            )
    ) {
        NovaIcon(
            imageVector = NovaIcons.ReplyArrow,
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .padding(PolkadotTheme.spacings.small)
                .graphicsLayer {
                    val arrowGraphicsMultiplier = (abs(offsetX.value) / swipeThresholdPx).coerceIn(0f, 1f)

                    scaleX = arrowGraphicsMultiplier
                    scaleY = arrowGraphicsMultiplier
                    alpha = arrowGraphicsMultiplier
                },
            tint = PolkadotTheme.colors.fg.secondary
        )

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .graphicsLayer {
                    translationX = offsetX.value
                },
            content = content
        )
    }
}
