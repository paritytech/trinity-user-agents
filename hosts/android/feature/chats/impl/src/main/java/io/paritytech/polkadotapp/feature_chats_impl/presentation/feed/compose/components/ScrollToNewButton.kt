package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDropdown
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.launch

@Composable
internal fun ScrollToNewButton(
    modifier: Modifier,
    lazyListState: LazyListState,
    unreadCounter: Int,
    containerColor: Color? = null,
    iconTint: Color? = null,
) {
    val scope = rememberCoroutineScope()

    val showButton by remember {
        derivedStateOf {
            lazyListState.scrolledToNewest
        }
    }

    AnimatedVisibility(
        modifier = modifier,
        visible = showButton,
        enter = slideInVertically { it } + fadeIn(),
        exit = slideOutVertically { it } + fadeOut()
    ) {
        Box {
            IconButton(
                modifier = Modifier
                    .padding(PolkadotTheme.spacings.mediumIncreased)
                    .size(48.dp),
                colors = IconButtonDefaults.iconButtonColors(
                    containerColor = containerColor ?: PolkadotTheme.colors.bg.surface.nested
                ),
                onClick = {
                    scope.launch {
                        if (lazyListState.scrolledTooFar) {
                            lazyListState.scrollToItem(TOO_FAR_INDEX)
                        }
                        lazyListState.animateScrollToItem(0)
                    }
                }
            ) {
                NovaIcon(
                    imageVector = NovaIcons.ArrowDropdown,
                    tint = iconTint ?: LocalContentColor.current,
                )
            }

            AnimatedVisibility(
                modifier = Modifier.align(Alignment.BottomEnd),
                visible = unreadCounter > 0,
                enter = scaleIn() + fadeIn(),
                exit = scaleOut() + fadeOut()
            ) {
                PolkadotSurface(
                    modifier = Modifier.padding(PolkadotTheme.spacings.small),
                    shape = PolkadotTheme.shapes.full,
                    color = PolkadotTheme.colors.fg.primary
                ) {
                    UnreadCounterText(unreadCounter)
                }
            }
        }
    }
}

@Composable
private fun UnreadCounterText(counter: Int) {
    AnimatedContent(
        label = "counter",
        targetState = counter,
        transitionSpec = {
            if (targetState > initialState) {
                slideInVertically { height -> height } + fadeIn() togetherWith
                    slideOutVertically { height -> -height } + fadeOut()
            } else {
                slideInVertically { height -> -height } + fadeIn() togetherWith
                    slideOutVertically { height -> height } + fadeOut()
            }.using(
                SizeTransform(clip = false)
            )
        }
    ) { targetCount ->
        NovaText(
            modifier = Modifier
                .widthIn(min = 24.dp)
                .padding(PolkadotTheme.spacings.extraTiny),
            text = if (targetCount > 99) "99+" else targetCount.toString(),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.bg.surface.main,
            textAlign = TextAlign.Center
        )
    }
}

private const val TOO_FAR_INDEX = 10

private val LazyListState.scrolledTooFar
    get() = firstVisibleItemIndex > TOO_FAR_INDEX

private val LazyListState.scrolledToNewest
    get() = firstVisibleItemIndex > 2
