package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButton
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components.icons.UserRound
import io.paritytech.polkadotapp.common.R as RCommon

private val IconContainerSize = 40.dp
private val IconSize = 20.dp

@Composable
internal fun SendToYourselfItem(
    modifier: Modifier = Modifier,
    onClick: () -> Unit
) {
    PolkadotButton(
        onClick = onClick,
        modifier = modifier,
        style = PolkadotButtonStyle.ghost(),
        size = PolkadotButtonSize.large(),
        shape = PolkadotTheme.shapes.mediumIncreased
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
            verticalAlignment = Alignment.CenterVertically
        ) {
            PolkadotSurface(
                modifier = Modifier.size(IconContainerSize),
                color = PolkadotTheme.colors.bg.action.tertiary,
                shape = PolkadotTheme.shapes.full,
                contentAlignment = Alignment.Center
            ) {
                NovaIcon(
                    modifier = Modifier.requiredSize(IconSize),
                    imageVector = UserRound,
                    tint = PolkadotTheme.colors.fg.primary
                )
            }

            Column {
                NovaText(
                    text = stringResource(RCommon.string.send_payment_send_to_yourself),
                    style = PolkadotTheme.typography.title.medium,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )

                NovaText(
                    text = stringResource(RCommon.string.send_payment_send_to_yourself_description),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
    }
}

@Preview
@Composable
private fun SendToYourselfItemPreview() {
    PolkadotTheme {
        SendToYourselfItem(onClick = {})
    }
}
