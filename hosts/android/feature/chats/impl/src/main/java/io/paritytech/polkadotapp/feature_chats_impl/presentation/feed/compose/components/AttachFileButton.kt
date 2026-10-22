package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Add
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun AttachFileButton(
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    PolkadotIconButton(
        modifier = modifier,
        icon = NovaIcons.Add,
        onClick = onClick,
        style = PolkadotButtonStyle.secondary(),
        size = PolkadotIconButtonSize.mediumIncreased(),
        shape = PolkadotTheme.shapes.full,
    )
}

@Preview
@Composable
private fun AttachFileButtonPreview() {
    PolkadotTheme {
        AttachFileButton { }
    }
}
