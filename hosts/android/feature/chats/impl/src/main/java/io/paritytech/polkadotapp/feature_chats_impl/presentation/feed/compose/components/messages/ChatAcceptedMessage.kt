package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.QuestionAnswer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ChatAcceptedMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.ChatAccepted
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaIcon(
            modifier = Modifier.size(24.dp),
            imageVector = NovaIcons.QuestionAnswer,
            tint = PolkadotTheme.colors.fg.tertiary
        )

        NovaText(
            text = stringResource(
                RCommon.string.chat_request_approved,
                message.peerUsername
            ).withBold(message.peerUsername),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatAcceptedMessagePreview() {
    PolkadotTheme {
        ChatAcceptedMessage(
            message = ChatMessageUiModel.ChatAccepted(
                id = "1",
                timestamp = System.currentTimeMillis(),
                direction = ChatMessageUiModel.Direction.INCOMING,
                status = ChatMessageUiModel.Status.READ,
                origin = ChatMessageOrigin.User,
                peerUsername = "Julius.87"
            )
        )
    }
}
