package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.HoldingGeometry

@Composable
internal fun CoinageWidgetCard(
    modifier: Modifier = Modifier,
    title: String,
    subtitle: String? = null,
    content: @Composable ColumnScope.() -> Unit
) {
    PolkadotSurface(
        modifier = modifier,
        shape = RoundedCornerShape(HoldingGeometry.containerCorner),
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(HoldingGeometry.containerPadding),
            verticalArrangement = Arrangement.spacedBy(HoldingGeometry.containerSpacing)
        ) {
            Column {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.title.medium,
                    color = PolkadotTheme.colors.fg.primary
                )

                if (subtitle != null) {
                    NovaText(
                        text = subtitle,
                        style = PolkadotTheme.typography.body.small,
                        color = PolkadotTheme.colors.fg.secondary
                    )
                }
            }

            content()
        }
    }
}
