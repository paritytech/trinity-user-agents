package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.separators

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatDateSeparatorStyle

private val DATE_PILL_RADIUS = 12.dp

@Composable
internal fun GroupDateSeparator(
    date: String,
    isHidden: Boolean,
    style: ChatDateSeparatorStyle? = null,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.small)
            .alpha(if (isHidden) 0f else 1f),
        contentAlignment = Alignment.Center
    ) {
        if (style == null) {
            NovaText(
                text = date,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.secondary
            )
        } else {
            PolkadotSurface(
                shape = RoundedCornerShape(DATE_PILL_RADIUS),
                color = style.backgroundColor,
            ) {
                NovaText(
                    modifier = Modifier.padding(
                        horizontal = PolkadotTheme.spacings.small,
                        vertical = PolkadotTheme.spacings.tiny,
                    ),
                    text = date,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = style.textColor
                )
            }
        }
    }
}
