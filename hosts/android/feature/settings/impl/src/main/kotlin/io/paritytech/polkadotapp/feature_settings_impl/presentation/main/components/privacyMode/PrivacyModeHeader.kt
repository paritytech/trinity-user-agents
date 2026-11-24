package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.icon.vectors.ShieldOutlined
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun PrivacyModeHeader(onInfoClick: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(
            modifier = Modifier
                .align(Alignment.Top)
                .padding(top = PolkadotTheme.spacings.extraSmall)
                .size(HEADER_ICON_SLOT_SIZE),
            contentAlignment = Alignment.Center
        ) {
            NovaIcon(
                modifier = Modifier.size(ICON_SIZE),
                imageVector = NovaIcons.ShieldOutlined,
                tint = PolkadotTheme.colors.fg.primary
            )
        }

        HorizontalSpacer { smallIncreased }

        Column(
            modifier = Modifier
                .weight(1f)
                .padding(vertical = PolkadotTheme.spacings.smallIncreased),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall)
        ) {
            NovaText(
                text = stringResource(RCommon.string.payment_privacy_mode_title, LocalPaymentAssetBrand.current.symbol)
                    .withCurrencyTickerStyle(PolkadotTheme.typography.body.large),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary
            )

            NovaText(
                text = stringResource(RCommon.string.payment_privacy_mode_subtitle),
                style = PolkadotTheme.typography.paragraph.medium,
                color = PolkadotTheme.colors.fg.secondary
            )
        }

        HorizontalSpacer { extraSmall }

        val infoDescription = stringResource(RCommon.string.payment_privacy_mode_coin_quality_title)
        PolkadotIconButton(
            modifier = Modifier.semantics { contentDescription = infoDescription },
            icon = NovaIcons.Info,
            onClick = onInfoClick,
            style = PolkadotButtonStyle.ghost(),
            size = PolkadotIconButtonSize.small(),
            shape = PolkadotButtonShape.pill
        )
    }
}

private val HEADER_ICON_SLOT_SIZE = 32.dp
private val ICON_SIZE = 20.dp
