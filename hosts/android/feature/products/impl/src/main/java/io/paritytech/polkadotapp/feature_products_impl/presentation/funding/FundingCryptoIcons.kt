package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.DrawableRes
import androidx.compose.ui.graphics.vector.ImageVector
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenDOT
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenUSDC
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenUSDT
import uniffi.truapi.fundingNetworkName
import java.util.Locale
import io.paritytech.polkadotapp.common.R as RCommon

/** The logo a crypto network or token is drawn with; one without a logo shows its first letter. */
sealed interface FundingCryptoIcon {
    data class Vector(val image: ImageVector) : FundingCryptoIcon

    data class Drawable(@DrawableRes val id: Int) : FundingCryptoIcon

    data class Monogram(val text: String) : FundingCryptoIcon
}

private val DOT = FundingCryptoIcon.Vector(NovaIcons.TokenDOT)
private val BITCOIN = FundingCryptoIcon.Drawable(RCommon.drawable.img_crypto_bitcoin)
private val ETHEREUM = FundingCryptoIcon.Drawable(RCommon.drawable.img_crypto_ethereum)
private val SOLANA = FundingCryptoIcon.Drawable(RCommon.drawable.img_crypto_solana)
private val TRON = FundingCryptoIcon.Drawable(RCommon.drawable.img_crypto_tron)

/** A network's logo by the id the providers' routes name it with. */
fun fundingNetworkIcon(id: String): FundingCryptoIcon = when (id.lowercase(Locale.ROOT)) {
    "polkadot" -> DOT
    "bitcoin" -> BITCOIN
    "ethereum" -> ETHEREUM
    "solana" -> SOLANA
    "tron" -> TRON
    else -> monogramOf(fundingNetworkName(id))
}

/** A token's logo by symbol; a network's native token shares the network's logo. */
fun fundingTokenIcon(symbol: String): FundingCryptoIcon = when (symbol.lowercase(Locale.ROOT)) {
    "usdc" -> FundingCryptoIcon.Vector(NovaIcons.TokenUSDC)
    "usdt" -> FundingCryptoIcon.Vector(NovaIcons.TokenUSDT)
    "dotusd" -> FundingCryptoIcon.Drawable(RCommon.drawable.img_crypto_dotusd)
    "dot" -> DOT
    "btc" -> BITCOIN
    "eth" -> ETHEREUM
    "sol" -> SOLANA
    "trx" -> TRON
    else -> monogramOf(symbol)
}

private fun monogramOf(name: String) = FundingCryptoIcon.Monogram(name.take(1).uppercase(Locale.ROOT))
