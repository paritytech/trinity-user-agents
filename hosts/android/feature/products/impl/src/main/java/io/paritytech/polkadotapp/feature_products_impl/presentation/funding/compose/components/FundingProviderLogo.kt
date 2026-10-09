package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand

/** The provider product's icon, or its initial while there is none. */
@Composable
fun FundingProviderLogo(
    brand: FundingProviderBrand,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    if (brand.iconUrl != null) {
        NovaAsyncImage(
            modifier = modifier
                .size(size)
                .clip(PolkadotTheme.shapes.full),
            model = brand.iconUrl,
            contentDescription = brand.name,
            contentScale = ContentScale.Crop,
        )
    } else {
        PolkadotSurface(
            modifier = modifier.size(size),
            shape = PolkadotTheme.shapes.full,
            color = PolkadotTheme.colors.bg.surface.nested,
            contentAlignment = Alignment.Center,
        ) {
            NovaText(
                text = brand.name.take(1).uppercase(),
                style = PolkadotTheme.typography.body.smallEmphasized,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}
