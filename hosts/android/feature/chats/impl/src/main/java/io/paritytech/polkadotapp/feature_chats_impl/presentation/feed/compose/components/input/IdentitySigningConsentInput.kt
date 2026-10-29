package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun IdentitySigningConsentInput(
    onAllow: () -> Unit,
    onDecline: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.mediumIncreased
            )
    ) {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_identity_signing_consent_title),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { tiny }

        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_identity_signing_consent_body),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { mediumIncreased }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium)
        ) {
            PolkadotTextButton(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.chat_identity_signing_consent_decline),
                onClick = onDecline,
                size = PolkadotButtonSize.largeIncreased(),
                style = PolkadotButtonStyle.destructive()
            )

            PolkadotTextButton(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.chat_identity_signing_consent_allow),
                onClick = onAllow,
                size = PolkadotButtonSize.largeIncreased(),
                style = PolkadotButtonStyle.secondary()
            )
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun IdentitySigningConsentInputPreview() {
    PolkadotTheme {
        IdentitySigningConsentInput(
            onAllow = {},
            onDecline = {}
        )
    }
}
