package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpRight
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

private val ICON_SIZE = 40.dp
private val GLYPH_SIZE = 20.dp
private val CHEVRON_SIZE = 16.dp

/** Sending value out of the pocket, to a bank, a card or a crypto wallet. */
@Composable
fun OutsidePocketRow(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    PolkadotSurface(
        modifier = modifier,
        color = PolkadotTheme.colors.bg.surface.main,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
        ) {
            PolkadotSurface(
                modifier = Modifier.size(ICON_SIZE),
                shape = PolkadotTheme.shapes.full,
                color = PolkadotTheme.colors.bg.surface.nested,
                contentAlignment = Alignment.Center,
            ) {
                NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.ArrowUpRight, tint = PolkadotTheme.colors.fg.primary)
            }

            Column(modifier = Modifier.weight(1f)) {
                NovaText(
                    text = stringResource(RCommon.string.funding_outside_pocket_title),
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                )
                NovaText(
                    text = stringResource(RCommon.string.funding_outside_pocket_subtitle),
                    style = PolkadotTheme.typography.body.small,
                    color = PolkadotTheme.colors.fg.secondary,
                )
            }

            NovaIcon(modifier = Modifier.size(CHEVRON_SIZE), imageVector = NovaIcons.ArrowRight, tint = PolkadotTheme.colors.fg.tertiary)
        }
    }
}
