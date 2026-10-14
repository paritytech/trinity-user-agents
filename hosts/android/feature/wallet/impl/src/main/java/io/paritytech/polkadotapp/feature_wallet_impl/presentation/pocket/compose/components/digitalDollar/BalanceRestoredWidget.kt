package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AlertFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun BalanceRestoredWidget(
    modifier: Modifier = Modifier,
    onWhyClick: () -> Unit,
    onCloseClick: () -> Unit,
    onUpdateClick: () -> Unit,
    isInProgress: Boolean
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased)
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                NovaIcon(
                    modifier = Modifier.size(20.dp),
                    imageVector = NovaIcons.AlertFilled,
                    tint = PolkadotTheme.colors.fg.warning
                )

                HorizontalSpacer { extraSmall }

                NovaText(
                    text = stringResource(RCommon.string.asset_details_balance_restored),
                    style = PolkadotTheme.typography.title.large,
                    color = PolkadotTheme.colors.fg.warning
                )
            }

            VerticalSpacer { small }

            NovaText(
                text = stringResource(RCommon.string.asset_details_balance_restored_description),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.primary
            )

            VerticalSpacer { small }

            Row(
                modifier = Modifier.clickable(onClick = onWhyClick),
                verticalAlignment = Alignment.CenterVertically
            ) {
                NovaText(
                    text = stringResource(RCommon.string.asset_details_balance_restored_why),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary
                )

                NovaIcon(
                    modifier = Modifier.size(20.dp),
                    imageVector = NovaIcons.ArrowRight,
                    tint = PolkadotTheme.colors.fg.secondary
                )
            }
            VerticalSpacer { mediumIncreased }

            Row {
                PolkadotTextButton(
                    modifier = Modifier.weight(1f),
                    text = stringResource(RCommon.string.asset_details_balance_CTA_close),
                    enabled = isInProgress.not(),
                    style = PolkadotButtonStyle.secondary(),
                    onClick = onCloseClick,
                    shape = PolkadotButtonShape.pill
                )

                HorizontalSpacer { small }

                PolkadotTextButton(
                    modifier = Modifier.weight(1f),
                    text = stringResource(RCommon.string.asset_details_balance_CTA_update),
                    enabled = isInProgress.not(),
                    style = PolkadotButtonStyle.primary(),
                    onClick = onUpdateClick,
                    shape = PolkadotButtonShape.pill
                )
            }
        }
    }
}

@Preview
@Composable
private fun BalanceRestoredWidgetPreview() {
    PolkadotTheme {
        BalanceRestoredWidget(
            onWhyClick = {},
            onCloseClick = {},
            onUpdateClick = {},
            isInProgress = false
        )
    }
}
