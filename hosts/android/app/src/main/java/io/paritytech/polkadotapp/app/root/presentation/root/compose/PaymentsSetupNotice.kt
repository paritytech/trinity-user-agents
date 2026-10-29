package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthBarDefaults
import io.paritytech.polkadotapp.common.R as RCommon

private val NoticeIconSize = 24.dp

@Composable
fun PaymentsSetupNotice(
    modifier: Modifier = Modifier,
    isVisible: Boolean,
    onDismissClick: () -> Unit,
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
                PaymentsSetupNoticeContent(onDismissClick = onDismissClick)
            }
        }
    }
}

@Composable
private fun PaymentsSetupNoticeContent(onDismissClick: () -> Unit) {
    val closeDescription = stringResource(RCommon.string.common_close)

    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            // The overlay sits above the screen's top bar, so a tap on the notice must not reach it.
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
        ) {
            NovaIcon(
                modifier = Modifier.size(NoticeIconSize),
                imageVector = NovaIcons.Info,
                tint = PolkadotTheme.colors.fg.secondary,
            )

            NovaText(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.payments_setup_notice_message),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.primary,
            )

            PolkadotIconButton(
                modifier = Modifier.semantics { contentDescription = closeDescription },
                icon = NovaIcons.Close,
                onClick = onDismissClick,
                style = PolkadotButtonStyle.ghost(),
                size = PolkadotIconButtonSize.small(),
                shape = PolkadotTheme.shapes.full,
            )
        }
    }
}

@Preview
@Composable
private fun PaymentsSetupNoticePreview() {
    PaymentsSetupNotice(
        modifier = Modifier.fillMaxSize(),
        isVisible = true,
        onDismissClick = {},
    )
}
