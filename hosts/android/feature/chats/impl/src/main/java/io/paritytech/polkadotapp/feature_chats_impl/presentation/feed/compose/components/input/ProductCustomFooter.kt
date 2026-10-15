package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer

/** The footer slot of a product room whose product draws the footer (`ChatRoomFooter::Custom`, 02-DESIGN §6.4). */
@Composable
internal fun ProductCustomFooter(
    modifier: Modifier = Modifier,
    renderer: CustomChatFooterRenderer,
    onHeightChanged: (Dp) -> Unit,
) {
    val density = LocalDensity.current

    PolkadotSurface(
        modifier = modifier
            .fillMaxWidth()
            .onSizeChanged { size -> onHeightChanged(with(density) { size.height.toDp() }) },
        color = PolkadotTheme.colors.bg.surface.main,
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .imePadding()
                .padding(PolkadotTheme.spacings.small)
        ) {
            renderer.drawFooter()
        }
    }
}
