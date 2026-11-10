package io.paritytech.polkadotapp.feature_chats_api.presentation

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val AVATAR_SIZE = 72.dp

@Composable
fun ChatHeaderAvatarAndName(
    username: String,
    avatarModel: AvatarUiModel,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        PolkadotAvatar(
            model = avatarModel,
            modifier = Modifier.size(AVATAR_SIZE)
        )

        VerticalSpacer { small }

        NovaText(
            text = username,
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )
    }
}

@Composable
fun ChatContactHeader(
    username: String,
    avatarModel: AvatarUiModel,
    modifier: Modifier = Modifier,
    subtitle: @Composable (ColumnScope.() -> Unit)? = null
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        PolkadotAvatar(
            model = avatarModel,
            modifier = Modifier.size(AVATAR_SIZE)
        )

        VerticalSpacer { small }

        NovaText(
            text = username,
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        subtitle?.let {
            VerticalSpacer { small }
            it()
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatHeaderAvatarAndNamePreview() {
    PolkadotTheme {
        ChatHeaderAvatarAndName(
            username = "Polkadot Peer Bot",
            avatarModel = AvatarUiModel.Mock.fromName("Polkadot Peer Bot")
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatContactHeaderPreview() {
    PolkadotTheme {
        ChatContactHeader(
            username = "brave beaver",
            avatarModel = AvatarUiModel.Mock.fromName("brave beaver"),
            subtitle = {
                NovaText(
                    text = "You both joined in January",
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                    textAlign = TextAlign.Center
                )
            }
        )
    }
}
