package io.paritytech.polkadotapp.design.components.topbar

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.More
import io.paritytech.polkadotapp.design.components.icon.vectors.VideocamFilled
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.persistentListOf

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun PolkadotTopBarPreview() {
    PolkadotTheme {
        Column {
            PolkadotTopBar(
                title = "Label",
                titleSize = TopBarTitleSize.Large,
                actions = persistentListOf(rememberTopBarAction(action = {}, icon = NovaIcons.More)),
            )

            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "Label",
                titleAlignment = TopBarTitleAlignment.Center,
                titleSize = TopBarTitleSize.Medium,
            )

            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "Label",
                subtitle = "Supporting text",
                titleAlignment = TopBarTitleAlignment.Center,
                actions = persistentListOf(rememberTopBarAction(action = {}, icon = NovaIcons.More)),
            )

            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "LongLabelWithManyWorks",
                subtitle = "Supporting text",
                titleAlignment = TopBarTitleAlignment.Center,
                actions = persistentListOf(
                    rememberTopBarAction(action = {}, icon = NovaIcons.CallFilled),
                    rememberTopBarAction(action = {}, icon = NovaIcons.VideocamFilled),
                    rememberTopBarAction(action = {}, icon = NovaIcons.More),
                ),
            )

            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "Label",
                subtitle = "Supporting text",
                actions = persistentListOf(
                    rememberTopBarAction(action = {}, icon = NovaIcons.VideocamFilled),
                    rememberTopBarAction(action = {}, icon = NovaIcons.More),
                ),
                leadingContent = {
                    PolkadotAvatar(
                        model = AvatarUiModel.Mock.fromName("Andrey"),
                        modifier = Modifier.fillMaxSize(),
                    )
                },
            )

            PolkadotTopBar {
                PolkadotSearchField(
                    modifier = Modifier.fillMaxWidth(),
                    value = "Input text",
                    onValueChange = {},
                    onClear = {},
                    placeholder = "Search",
                    leadingIcon = NovaIcons.ArrowLeft,
                    onLeadingClick = {},
                )
            }

            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
            )
        }
    }
}
