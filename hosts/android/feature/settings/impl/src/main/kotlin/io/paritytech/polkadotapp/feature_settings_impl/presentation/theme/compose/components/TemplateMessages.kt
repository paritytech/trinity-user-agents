package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun TemplateMessages(messageDrawer: TextMessageDrawer) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.large),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large)
    ) {
        messageDrawer.Draw(
            modifier = Modifier
                .widthIn(max = 200.dp)
                .align(Alignment.End),
            text = stringResource(RCommon.string.settings_theme_preview_message_first),
            isOutgoing = true
        )

        messageDrawer.Draw(
            modifier = Modifier
                .widthIn(max = 200.dp)
                .align(Alignment.Start),
            text = stringResource(RCommon.string.settings_theme_preview_message_second),
            repliedTo = stringResource(RCommon.string.common_you),
            repliedText = stringResource(RCommon.string.settings_theme_preview_message_first),
            isOutgoing = false
        )

        messageDrawer.Draw(
            modifier = Modifier
                .widthIn(max = 200.dp)
                .align(Alignment.End),
            text = stringResource(RCommon.string.settings_theme_preview_message_third),
            isOutgoing = true
        )
    }
}
