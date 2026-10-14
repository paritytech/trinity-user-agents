package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun InviteToChatLabel(
    username: String,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
    ) {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_request_invite_title, username),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )

        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_request_invite_description),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun InviteToChatLabelPreview() {
    PolkadotTheme {
        InviteToChatLabel(username = "Julius.87")
    }
}
