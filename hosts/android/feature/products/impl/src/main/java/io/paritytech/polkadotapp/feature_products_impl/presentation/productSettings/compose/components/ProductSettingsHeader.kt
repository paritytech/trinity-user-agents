package io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.models.ProductSettingsUiModel

@Composable
internal fun ProductSettingsHeader(uiModel: ProductSettingsUiModel) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        if (uiModel.iconUrl != null) {
            NovaAsyncImage(
                modifier = Modifier
                    .size(80.dp)
                    .clip(PolkadotTheme.shapes.mediumIncreased),
                model = uiModel.iconUrl,
                contentScale = ContentScale.Crop,
            )
        } else {
            PolkadotSurface(
                modifier = Modifier.size(80.dp),
                shape = PolkadotTheme.shapes.mediumIncreased,
                color = PolkadotTheme.colors.fg.secondary
            ) {}
        }

        VerticalSpacer { extraMedium }

        NovaText(
            text = uiModel.name,
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}
