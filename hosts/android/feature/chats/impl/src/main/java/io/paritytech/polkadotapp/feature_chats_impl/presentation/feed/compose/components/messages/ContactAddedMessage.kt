package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AccountsOutlined
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ContactAddedMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.ContactAdded,
    username: String
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaIcon(
            modifier = Modifier.size(20.dp),
            imageVector = NovaIcons.AccountsOutlined,
            tint = PolkadotTheme.colors.fg.secondary
        )

        NovaText(
            text = stringResource(
                when (message.direction) {
                    ChatMessageUiModel.Direction.INCOMING -> RCommon.string.chat_message_contact_added_incoming
                    ChatMessageUiModel.Direction.OUTGOING -> RCommon.string.chat_message_contact_added_outgoing
                },
                username
            ).withBold(username),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )
    }
}
