package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthBarDefaults

@Composable
fun TopNotice(
    modifier: Modifier = Modifier,
    isVisible: Boolean,
    content: @Composable RowScope.() -> Unit,
) {
    PolkadotTheme {
        Box(modifier = modifier) {
            AnimatedVisibility(
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .statusBarsPadding()
                    .padding(top = ChainHealthBarDefaults.ContentHeight)
                    .padding(
                        horizontal = PolkadotTheme.spacings.mediumIncreased,
                        vertical = PolkadotTheme.spacings.small,
                    ),
                visible = isVisible,
                enter = slideInVertically(initialOffsetY = { -it }) + fadeIn(),
                exit = slideOutVertically(targetOffsetY = { -it }) + fadeOut(),
            ) {
                PolkadotSurface(
                    modifier = Modifier
                        .fillMaxWidth()
                        .pointerInput(Unit) {},
                    color = PolkadotTheme.colors.bg.action.tertiary,
                    shape = PolkadotTheme.shapes.extraMedium,
                ) {
                    Row(
                        modifier = Modifier.padding(
                            start = PolkadotTheme.spacings.mediumIncreased,
                            top = PolkadotTheme.spacings.small,
                            bottom = PolkadotTheme.spacings.small,
                            end = PolkadotTheme.spacings.extraTiny,
                        ),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
                        content = content,
                    )
                }
            }
        }
    }
}
