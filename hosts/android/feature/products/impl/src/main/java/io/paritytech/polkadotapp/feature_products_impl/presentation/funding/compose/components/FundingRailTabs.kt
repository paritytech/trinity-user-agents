package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.annotation.DrawableRes
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingRailTab
import kotlinx.collections.immutable.ImmutableList
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

private val PILL_HEIGHT = 40.dp
private val PILL_PADDING = PaddingValues(start = 8.dp, end = 12.dp)
private val ICON_SIZE = 24.dp
private const val DISABLED_ALPHA = 0.35f
private val GREYSCALE = ColorFilter.colorMatrix(ColorMatrix().apply { setToSaturation(0f) })

/**
 * Crypto, Card and Bank as pills with the rail's icon. A rail no provider serves for this direction is shown dimmed and
 * grey, and cannot be picked.
 */
@Composable
fun FundingRailTabs(
    modifier: Modifier = Modifier,
    tabs: ImmutableList<FundingRailTab>,
    onSelect: (FundingRail) -> Unit,
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        tabs.forEach { tab ->
            PolkadotSurface(
                modifier = Modifier
                    .height(PILL_HEIGHT)
                    .alpha(if (tab.enabled) 1f else DISABLED_ALPHA),
                shape = PolkadotTheme.shapes.full,
                color = if (tab.selected) PolkadotTheme.colors.bg.surface.containerInverted else PolkadotTheme.colors.bg.surface.container,
                contentAlignment = Alignment.Center,
                enabled = tab.enabled,
                onClick = { onSelect(tab.rail) },
            ) {
                Row(
                    modifier = Modifier.padding(PILL_PADDING),
                    horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Image(
                        modifier = Modifier
                            .size(ICON_SIZE)
                            .clip(CircleShape),
                        painter = painterResource(tab.rail.iconRes()),
                        contentDescription = null,
                        colorFilter = if (tab.enabled) null else GREYSCALE,
                    )
                    NovaText(
                        text = stringResource(tab.rail.titleRes()),
                        style = PolkadotTheme.typography.body.mediumEmphasized,
                        color = if (tab.selected) PolkadotTheme.colors.fg.primaryInverted else PolkadotTheme.colors.fg.primary,
                    )
                }
            }
        }
    }
}

fun FundingRail.titleRes(): Int = when (this) {
    FundingRail.CRYPTO -> RCommon.string.funding_rail_crypto
    FundingRail.CARD -> RCommon.string.funding_rail_card
    FundingRail.BANK -> RCommon.string.funding_rail_bank
}

/** The rail's icon, rasterised from the web app's, which draws with blurs a vector drawable cannot. */
@DrawableRes
fun FundingRail.iconRes(): Int = when (this) {
    FundingRail.CRYPTO -> RCommon.drawable.img_funding_rail_crypto
    FundingRail.CARD -> RCommon.drawable.img_funding_rail_card
    FundingRail.BANK -> RCommon.drawable.img_funding_rail_bank
}
