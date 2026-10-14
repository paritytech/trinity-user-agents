package io.paritytech.polkadotapp.design.components.avatar

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

enum class NovaContactItemType {
    User, Address
}

@Composable
fun NovaContactItem(
    modifier: Modifier = Modifier,
    title: String,
    type: NovaContactItemType,
    avatarModel: AvatarUiModel,
    onClick: () -> Unit,
    endContent: @Composable (() -> Unit)? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                vertical = PolkadotTheme.spacings.extraMedium,
                horizontal = PolkadotTheme.spacings.mediumIncreased
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        when (type) {
            NovaContactItemType.User ->
                PolkadotAvatar(
                    modifier = Modifier
                        .size(48.dp),
                    model = avatarModel,
                )
            NovaContactItemType.Address -> NovaAddressAvatar(48.dp)
        }

        HorizontalSpacer { mediumIncreased }

        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )

        if (endContent != null) {
            HorizontalSpacer { extraMedium }
            endContent()
        }
    }
}
