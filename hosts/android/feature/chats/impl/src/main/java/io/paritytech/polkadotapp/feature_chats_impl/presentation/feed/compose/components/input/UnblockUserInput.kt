package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
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
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun UnblockUserInput(
    username: String,
    onUnblock: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.large,
                vertical = PolkadotTheme.spacings.mediumIncreased
            ),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            text = stringResource(RCommon.string.chat_blocked_user_notice, username).withBold(username),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { extraMedium }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.blocked_users_unblock),
            onClick = onUnblock,
            size = PolkadotButtonSize.largeIncreased(),
            style = PolkadotButtonStyle.destructive()
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun UnblockUserInputPreview() {
    PolkadotTheme {
        UnblockUserInput(
            username = "julius.87",
            onUnblock = {}
        )
    }
}
