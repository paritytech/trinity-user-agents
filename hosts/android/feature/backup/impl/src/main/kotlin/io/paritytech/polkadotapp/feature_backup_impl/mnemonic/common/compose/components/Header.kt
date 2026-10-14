package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.common.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun MnemonicHeader(
    title: String,
    description: String
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.large),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            text = title,
            style = PolkadotTheme.typography.headline.large,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { extraMedium }

        NovaText(
            text = description,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )
    }
}
