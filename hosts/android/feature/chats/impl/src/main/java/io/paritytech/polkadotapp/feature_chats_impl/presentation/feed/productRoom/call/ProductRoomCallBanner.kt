package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.VideocamFilled
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOverlayRenderer
import javax.inject.Inject

/**
 * The in-app banner of a product room that rings while another screen of the app is in front (07-LATER §11.2). A tap
 * opens the room, where the banner of the room answers the call. The app draws it with the overlays of the chat.
 */
class ProductRoomCallBannerRenderer @Inject constructor(
    private val alerts: ProductRoomCallAlerts,
    private val intents: ProductRoomCallIntents,
) : CustomChatOverlayRenderer {
    @Composable
    override fun DrawOverlay() {
        val banner by alerts.bannerState.collectAsStateWithLifecycle()
        val context = LocalContext.current
        banner?.let { ringing ->
            ProductRoomCallBanner(banner = ringing, onOpen = { context.startActivity(intents.openRoomIntent(ringing.chatId)) })
        }
    }
}

@Composable
private fun ProductRoomCallBanner(banner: ProductRoomCallBannerUiModel, onOpen: () -> Unit) {
    PolkadotSurface(
        // The overlay is a compose root of its own, so it gives its tags to the accessibility tree itself.
        modifier = Modifier
            .semantics { testTagsAsResourceId = true }
            .testTag(ProductRoomCallTags.IN_APP_BANNER)
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onOpen),
        color = PolkadotTheme.colors.bg.surface.container,
        shape = RoundedCornerShape(BannerCorner),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large, vertical = PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
        ) {
            NovaIcon(
                modifier = Modifier.size(BannerIconSize),
                imageVector = if (banner.video) NovaIcons.VideocamFilled else NovaIcons.CallFilled,
                tint = PolkadotTheme.colors.fg.success,
            )
            Column(modifier = Modifier.weight(1f)) {
                NovaText(
                    text = stringResource(ringingTitleRes(banner.video)),
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                )
                NovaText(
                    text = banner.roomName,
                    style = PolkadotTheme.typography.paragraph.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

private val BannerCorner = 28.dp
private val BannerIconSize = 24.dp
