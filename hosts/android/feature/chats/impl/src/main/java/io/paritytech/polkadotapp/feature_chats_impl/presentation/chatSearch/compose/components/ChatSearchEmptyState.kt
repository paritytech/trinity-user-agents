package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Search
import io.paritytech.polkadotapp.design.components.icon.vectors.SearchX
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ChatSearchNoResults(query: String) {
    ChatSearchEmptyState(icon = NovaIcons.SearchX) {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_search_empty_state_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )

        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_search_empty_state_message, query),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
internal fun ChatSearchPrompt() {
    ChatSearchEmptyState(icon = NovaIcons.Search) {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.chat_search_prompt_message),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun ChatSearchEmptyState(
    icon: ImageVector,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = PolkadotTheme.spacings.large),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaIcon(
            modifier = Modifier.size(64.dp),
            imageVector = icon,
            tint = PolkadotTheme.colors.fg.disabled,
        )

        VerticalSpacer { small }

        content()
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ChatSearchEmptyStatePreview() {
    PolkadotTheme {
        Row {
            Box(modifier = Modifier.weight(1f)) {
                ChatSearchPrompt()
            }

            Box(modifier = Modifier.weight(1f)) {
                ChatSearchNoResults(query = "Wqwewfr")
            }
        }
    }
}
