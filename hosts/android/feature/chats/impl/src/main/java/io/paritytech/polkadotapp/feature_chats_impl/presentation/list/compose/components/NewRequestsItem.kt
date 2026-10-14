package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.MessageUnreadOutlined
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.ChatTestTags
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun NewRequestsItem(
    modifier: Modifier = Modifier,
    count: Int,
    onClick: () -> Unit
) {
    PolkadotSurface(
        modifier = modifier.testTag(ChatTestTags.NEW_REQUESTS_ITEM),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = PolkadotTheme.spacings.extraMedium),
            verticalAlignment = Alignment.CenterVertically
        ) {
            HorizontalSpacer { 20.dp }

            NovaIcon(
                imageVector = NovaIcons.MessageUnreadOutlined,
                tint = PolkadotTheme.colors.fg.primary
            )

            HorizontalSpacer { mediumIncreased }

            NovaText(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.chat_request_new_requests),
                style = PolkadotTheme.typography.title.medium,
                color = PolkadotTheme.colors.fg.primary
            )

            UnreadBadge(count = count)

            HorizontalSpacer { small }

            NovaIcon(
                modifier = Modifier.size(16.dp),
                imageVector = NovaIcons.ArrowRight,
                tint = PolkadotTheme.colors.fg.secondary
            )

            HorizontalSpacer { mediumIncreased }
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun NewRequestsItemPreview() {
    PolkadotTheme {
        NewRequestsItem(
            count = 4,
            onClick = {}
        )
    }
}
