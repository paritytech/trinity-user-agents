package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun UnreadBadge(
    count: Int
) {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.fg.staticWhite,
        contentAlignment = Alignment.Center
    ) {
        NovaText(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.small),
            text = count.toString(),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = Color(0xFF000000)
        )
    }
}

@Preview
@Composable
private fun UnreadBadgePreview() {
    PolkadotTheme {
        Row(
            modifier = Modifier
                .background(Color.Black)
                .padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            UnreadBadge(count = 3)
            UnreadBadge(count = 99)
        }
    }
}
