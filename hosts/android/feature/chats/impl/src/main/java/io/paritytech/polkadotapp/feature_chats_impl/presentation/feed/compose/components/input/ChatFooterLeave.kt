package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Leave
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ChatFooterLeave(
    username: String
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.mediumIncreased),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaIcon(
            modifier = Modifier.size(24.dp),
            imageVector = NovaIcons.Leave,
            tint = PolkadotTheme.colors.fg.tertiary
        )

        HorizontalSpacer { small }

        NovaText(
            text = stringResource(RCommon.string.chat_user_left_chat, username).withBold(username),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatFooterLeavePreview() {
    PolkadotTheme {
        ChatFooterLeave(
            username = "juliuslongname.87"
        )
    }
}
