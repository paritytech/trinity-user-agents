package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ChatFooterRequestCancelled(
    username: String
) {
    NovaText(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
                top = PolkadotTheme.spacings.mediumIncreased,
                bottom = PolkadotTheme.spacings.large
            ),
        text = stringResource(RCommon.string.chat_request_cancelled_by_peer, username).withBold(username),
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.tertiary,
        textAlign = TextAlign.Center
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatFooterRequestCancelledPreview() {
    PolkadotTheme {
        ChatFooterRequestCancelled(username = "juliuslongname.87")
    }
}
