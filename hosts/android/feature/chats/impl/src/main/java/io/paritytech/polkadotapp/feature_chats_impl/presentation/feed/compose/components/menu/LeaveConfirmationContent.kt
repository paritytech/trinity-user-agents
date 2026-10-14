package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

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
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun LeaveConfirmationContent(
    type: ChatMenuType.LeaveConfirmation,
    onConfirm: () -> Unit,
    onCancel: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.large),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            text = stringResource(RCommon.string.chat_leave_chat_question),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(
                RCommon.string.chat_leave_chat_confirmation,
                type.username
            ).withBold(type.username),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { large }

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            val buttonModifier = Modifier.weight(1f)

            PolkadotTextButton(
                modifier = buttonModifier,
                text = stringResource(RCommon.string.common_cancel),
                style = PolkadotButtonStyle.secondary(),
                onClick = onCancel
            )

            PolkadotTextButton(
                modifier = buttonModifier,
                text = stringResource(RCommon.string.common_delete),
                style = PolkadotButtonStyle.destructive(),
                onClick = onConfirm
            )
        }
    }
}
