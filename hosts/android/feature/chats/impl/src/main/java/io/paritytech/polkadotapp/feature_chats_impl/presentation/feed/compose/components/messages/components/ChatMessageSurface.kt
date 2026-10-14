package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalHapticFeedback
import io.paritytech.polkadotapp.common.utils.longPressIgnoreChildren
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ChatMessageSurface(
    modifier: Modifier = Modifier,
    direction: ChatMessageUiModel.Direction,
    grouping: ChatMessageGrouping = ChatMessageGrouping.Standalone,
    style: ChatMessageSurfaceStyle = ChatMessageSurfaceStyle.default(direction),
    onLongPress: ((MessageLayoutInfo) -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit
) {
    val shape = when {
        style == ChatMessageSurfaceStyle.Transparent -> PolkadotTheme.shapes.zero
        !grouping.isTopAttached && !grouping.isBottomAttached -> PolkadotTheme.shapes.mediumIncreased
        else -> bubbleGroupedShape(direction = direction, grouping = grouping)
    }

    val coordinatesRef = rememberCoordinatesHolder()
    val currentOnLongPress by rememberUpdatedState(onLongPress)
    val haptics = LocalHapticFeedback.current

    val clickModifier = if (onLongPress != null) {
        Modifier
            .onGloballyPositioned { coordinatesRef.value = it }
            .pointerInput(Unit) {
                longPressIgnoreChildren {
                    val coords = coordinatesRef.value
                    if (coords != null && coords.isAttached) {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        currentOnLongPress?.invoke(MessageLayoutInfo(coords.positionInWindow(), coords.size))
                    }
                }
            }
    } else {
        Modifier
    }

    PolkadotSurface(
        modifier = modifier.then(clickModifier),
        shape = shape,
        color = style.backgroundColor,
        border = style.border,
        content = content
    )
}

@Composable
private fun bubbleGroupedShape(
    direction: ChatMessageUiModel.Direction,
    grouping: ChatMessageGrouping
): Shape {
    val full = PolkadotTheme.radii.mediumIncreased
    val tail = PolkadotTheme.radii.small

    val topInside = if (grouping.isTopAttached) tail else full
    val bottomInside = if (grouping.isBottomAttached) tail else full

    return when (direction) {
        ChatMessageUiModel.Direction.INCOMING -> RoundedCornerShape(
            topStart = topInside,
            topEnd = full,
            bottomStart = bottomInside,
            bottomEnd = full
        )
        ChatMessageUiModel.Direction.OUTGOING -> RoundedCornerShape(
            topStart = full,
            topEnd = topInside,
            bottomStart = full,
            bottomEnd = bottomInside
        )
    }
}

// Holder for gesture-only layout data; avoid reading in composition to prevent recompositions.
private class CoordinatesHolder {
    var value: LayoutCoordinates? = null
}

@Composable
private fun rememberCoordinatesHolder(): CoordinatesHolder = remember { CoordinatesHolder() }
