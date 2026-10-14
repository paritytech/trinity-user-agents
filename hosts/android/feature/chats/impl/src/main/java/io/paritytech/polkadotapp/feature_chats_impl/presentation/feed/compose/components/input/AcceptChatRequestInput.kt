package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.ChatTestTags
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatRequestAnswerProgress
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun AcceptChatRequestInput(
    username: String,
    answerProgress: ChatRequestAnswerProgress,
    onAccept: () -> Unit,
    onDecline: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isAnswering = answerProgress != ChatRequestAnswerProgress.None
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.mediumIncreased
            )
    ) {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_request_accept_title, username),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { tiny }

        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_request_accept_description, username),
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
                modifier = Modifier.weight(1f).testTag(ChatTestTags.CHAT_DECLINE_BUTTON),
                text = stringResource(RCommon.string.chat_request_decline),
                onClick = onDecline,
                enabled = !isAnswering,
                loading = answerProgress == ChatRequestAnswerProgress.Declining,
                size = PolkadotButtonSize.largeIncreased(),
                style = PolkadotButtonStyle.destructive()
            )

            PolkadotTextButton(
                modifier = Modifier.weight(1f).testTag(ChatTestTags.CHAT_ACCEPT_BUTTON),
                text = stringResource(RCommon.string.chat_request_accept),
                onClick = onAccept,
                enabled = !isAnswering,
                loading = answerProgress == ChatRequestAnswerProgress.Accepting,
                size = PolkadotButtonSize.largeIncreased(),
                style = PolkadotButtonStyle.secondary()
            )
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun AcceptChatRequestInputPreview() {
    PolkadotTheme {
        AcceptChatRequestInput(
            username = "Maxwell.42",
            answerProgress = ChatRequestAnswerProgress.None,
            onAccept = {},
            onDecline = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun AcceptChatRequestInputPreview_Accepting() {
    PolkadotTheme {
        AcceptChatRequestInput(
            username = "Maxwell.42",
            answerProgress = ChatRequestAnswerProgress.Accepting,
            onAccept = {},
            onDecline = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun AcceptChatRequestInputPreview_Declining() {
    PolkadotTheme {
        AcceptChatRequestInput(
            username = "Maxwell.42",
            answerProgress = ChatRequestAnswerProgress.Declining,
            onAccept = {},
            onDecline = {}
        )
    }
}
