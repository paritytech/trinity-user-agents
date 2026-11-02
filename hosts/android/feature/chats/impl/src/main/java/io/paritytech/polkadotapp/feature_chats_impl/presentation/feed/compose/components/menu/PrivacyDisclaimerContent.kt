package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ShieldBroken
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PrivacyDisclaimerContent(
    onDismiss: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.large),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaIcon(
            modifier = Modifier.size(32.dp),
            imageVector = NovaIcons.ShieldBroken,
            tint = PolkadotTheme.colors.fg.warning,
        )

        VerticalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(RCommon.string.chat_privacy_disclaimer_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(RCommon.string.chat_privacy_disclaimer_body),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { large }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_got_it),
            style = PolkadotButtonStyle.secondary(),
            onClick = onDismiss
        )
    }
}

@Preview
@Composable
private fun PrivacyDisclaimerContentPreview() {
    PolkadotTheme {
        PrivacyDisclaimerContent(onDismiss = {})
    }
}
