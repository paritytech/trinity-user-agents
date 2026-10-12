package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.id

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel

@Composable
internal fun BasicIdCard(
    modifier: Modifier,
    card: PocketCardUiModel.IdCard,
    onSelected: ((PocketCardUiModel.IdCard) -> Unit)?,
    onQrClick: () -> Unit
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PocketCardColors.Transparent,
        border = BorderStroke(PolkadotTheme.borders.default, PocketCardColors.Primary),
        onClick = { onSelected?.invoke(card) },
        enabled = onSelected != null
    ) {
        Image(
            modifier = Modifier.matchParentSize(),
            painter = painterResource(R.drawable.basic_card_background),
            contentDescription = null,
            contentScale = ContentScale.Crop
        )

        IdCardContent(
            username = card.username,
            avatarPainter = painterResource(R.drawable.basic_card_placeholder),
            primaryTextColor = PocketCardColors.Secondary,
            onQrClick = onQrClick
        )
    }
}
