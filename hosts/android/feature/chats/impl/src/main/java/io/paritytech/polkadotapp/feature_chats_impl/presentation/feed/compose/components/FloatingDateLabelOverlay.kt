package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatDateSeparatorStyle
import kotlinx.coroutines.delay

internal data class StickyHeaderInfo(
    val formattedDate: String,
    val messageIndex: Int // Index of the message whose date group this separator belongs to
)

@Composable
internal fun BoxScope.FloatingDateLabelOverlay(
    stickyHeaderInfo: State<StickyHeaderInfo?>,
    lazyListState: LazyListState,
    style: ChatDateSeparatorStyle? = null,
) {
    val info = stickyHeaderInfo.value
    val isScrolling = lazyListState.isScrollInProgress

    var stickyHeaderVisible by remember { mutableStateOf(false) }

    LaunchedEffect(isScrolling) {
        if (isScrolling) {
            stickyHeaderVisible = true
        } else {
            delay(1000)
            stickyHeaderVisible = false
        }
    }

    Box(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .padding(top = PolkadotTheme.spacings.small)
    ) {
        AnimatedVisibility(
            visible = stickyHeaderVisible && info != null,
            enter = fadeIn(animationSpec = tween(200)),
            exit = fadeOut(animationSpec = tween(200))
        ) {
            info?.let { stickyHeader ->
                FloatingDateLabel(date = stickyHeader.formattedDate, style = style)
            }
        }
    }
}

@Composable
private fun FloatingDateLabel(
    modifier: Modifier = Modifier,
    date: String,
    style: ChatDateSeparatorStyle? = null,
) {
    PolkadotSurface(
        modifier = modifier,
        color = style?.backgroundColor ?: PolkadotTheme.colors.bg.surface.container,
        shape = PolkadotTheme.shapes.extraLarge,
        contentAlignment = Alignment.Center
    ) {
        Box(
            modifier = Modifier
                .padding(horizontal = 12.dp, vertical = 4.dp)
        ) {
            NovaText(
                text = date,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = style?.textColor ?: PolkadotTheme.colors.fg.tertiary
            )
        }
    }
}
