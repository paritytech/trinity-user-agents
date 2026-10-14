package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
fun DeclineConfirmationContent(
    onCancel: () -> Unit,
    onConfirm: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.large),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            text = stringResource(RCommon.string.chat_request_decline_dialog_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { extraLargeIncreased }

        Row(
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            val modifier = Modifier.weight(1f)

            PolkadotTextButton(
                modifier = modifier,
                text = stringResource(RCommon.string.common_cancel),
                style = PolkadotButtonStyle.secondary(),
                onClick = onCancel
            )

            PolkadotTextButton(
                modifier = modifier,
                text = stringResource(RCommon.string.common_decline),
                style = PolkadotButtonStyle.destructive(),
                onClick = onConfirm
            )
        }
    }
}
