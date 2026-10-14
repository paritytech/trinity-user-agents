package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold

@Composable
internal fun SendPaymentEmptySearch(text: String) {
    Box(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.Center
    ) {
        NovaText(
            text = stringResource(
                id = R.string.common_no_results_for,
                text
            )
                .withBold(text),
            style = PolkadotTheme.typography.title.medium,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )
    }
}
