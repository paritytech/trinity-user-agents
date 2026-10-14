package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun EnterAmountInputRow(
    input: String,
    symbol: String,
    showError: Boolean,
    onInputChange: (String) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            text = symbol,
            style = PolkadotTheme.typography.display.extraLarge,
            color = PolkadotTheme.colors.fg.tertiary
        )

        HorizontalSpacer { tiny }

        BasicTextField(
            modifier = Modifier.width(IntrinsicSize.Min),
            value = input,
            onValueChange = onInputChange,
            singleLine = true,
            textStyle = PolkadotTheme.typography.display.extraLarge.copy(
                color = PolkadotTheme.colors.fg.primary
            ),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        )
    }

    VerticalSpacer { small }

    if (showError) {
        NovaText(
            text = stringResource(R.string.send_enter_amount_not_enough_funds_error),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.error,
            textAlign = TextAlign.Companion.Center
        )
    }
}
