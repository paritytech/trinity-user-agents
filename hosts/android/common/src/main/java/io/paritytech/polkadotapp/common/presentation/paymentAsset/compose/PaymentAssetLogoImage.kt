package io.paritytech.polkadotapp.common.presentation.paymentAsset.compose

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.layout.ContentScale
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetLogo
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetLogoVariant
import io.paritytech.polkadotapp.common.presentation.paymentAsset.compose.icons.BundledPaymentAssetLogos
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage

/**
 * Draws the brand's logo for [variant] inside the caller's [modifier] bounds; the bundled mark is shown while a
 * published logo loads or when it fails. Callers that fix only one dimension add [aspectRatio] themselves.
 */
@Composable
fun PaymentAssetLogoImage(
    modifier: Modifier = Modifier,
    variant: PaymentAssetLogoVariant,
) {
    val brand = LocalPaymentAssetBrand.current
    val bundled = rememberVectorPainter(variant.bundled)

    NovaAsyncImage(
        modifier = modifier,
        model = (brand.logo(variant) as? PaymentAssetLogo.Remote)?.url,
        contentDescription = null,
        placeholder = bundled,
        error = bundled,
        contentScale = ContentScale.Fit,
    )
}

/** Width to height of the bundled mark for [this] variant; published logos are letterboxed into the same box. */
val PaymentAssetLogoVariant.aspectRatio: Float
    get() = bundled.defaultWidth / bundled.defaultHeight

private val PaymentAssetLogoVariant.bundled: ImageVector
    get() = when (this) {
        PaymentAssetLogoVariant.Square -> BundledPaymentAssetLogos.Square
        PaymentAssetLogoVariant.Wide -> BundledPaymentAssetLogos.Wide
    }

private fun PaymentAssetBrand.logo(variant: PaymentAssetLogoVariant): PaymentAssetLogo = when (variant) {
    PaymentAssetLogoVariant.Square -> squareLogo
    PaymentAssetLogoVariant.Wide -> wideLogo
}
