package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ErrorContent(onRejectClicked: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        VerticalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(RCommon.string.sign_transaction_failed_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { extraLarge }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_close),
            style = PolkadotButtonStyle.secondary(),
            onClick = onRejectClicked
        )
    }
}
