package io.paritytech.polkadotapp.feature_products_impl.presentation.list.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.list.models.ProductListItemUiModel

@Composable
internal fun ProductItem(
    item: ProductListItemUiModel,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .clickable(onClick = onClick)
            .padding(
                vertical = PolkadotTheme.spacings.extraMedium,
                horizontal = PolkadotTheme.spacings.large
            ),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (item.iconUrl != null) {
            NovaAsyncImage(
                modifier = Modifier
                    .size(32.dp)
                    .clip(PolkadotTheme.shapes.small),
                model = item.iconUrl,
                contentScale = ContentScale.Crop,
            )
        } else {
            PolkadotSurface(
                modifier = Modifier.size(32.dp),
                shape = PolkadotTheme.shapes.small,
                color = PolkadotTheme.colors.fg.secondary
            ) {}
        }

        NovaText(
            modifier = Modifier.weight(1f),
            text = item.name,
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.primary
        )

        NovaIcon(
            imageVector = NovaIcons.ArrowRight
        )
    }
}
