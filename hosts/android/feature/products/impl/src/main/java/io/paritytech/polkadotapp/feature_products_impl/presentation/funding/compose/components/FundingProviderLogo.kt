package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand

@Composable
fun FundingProviderLogo(
    brand: FundingProviderBrand,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    val model = remember(brand) {
        val initial = AvatarUiModel.Name(brand.name, AvatarColorScheme.from(brand.name.encodeToByteArray()))
        brand.iconUrl?.let { AvatarUiModel.Image(it, placeholder = initial, refresh = false) } ?: initial
    }
    PolkadotAvatar(model = model, modifier = modifier.size(size))
}
