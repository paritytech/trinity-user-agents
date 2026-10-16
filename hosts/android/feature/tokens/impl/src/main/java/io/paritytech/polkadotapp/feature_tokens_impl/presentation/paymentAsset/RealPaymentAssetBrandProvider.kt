package io.paritytech.polkadotapp.feature_tokens_impl.presentation.paymentAsset

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrandProvider
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetLogo
import io.paritytech.polkadotapp.common.utils.CurrencyConfig
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_tokens_impl.data.paymentAsset.PaymentAssetConfigProvider
import io.paritytech.polkadotapp.feature_tokens_impl.domain.paymentAsset.PaymentAssetConfig
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class RealPaymentAssetBrandProvider @Inject constructor(
    private val configProvider: PaymentAssetConfigProvider,
) : PaymentAssetBrandProvider, AppInitializer {
    override val brand = MutableStateFlow(PaymentAssetBrand.bundled(CurrencyConfig.defaultSymbol))

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        scope.launch { applyPublishedBrand() }
    }

    suspend fun applyPublishedBrand() {
        // Firebase keeps the last activated config on disk: applying it first keeps the published brand through an
        // offline session and avoids a bundled → published flash on every start.
        configProvider.lastActivatedPaymentAssetConfig()
            .logFailure("Last activated payment asset config unreadable, waiting for the sync")
            .onSuccess { config -> brand.value = config.toBrand() }

        configProvider.paymentAssetConfig()
            .logFailure("Payment asset config unavailable, keeping the current brand")
            .onSuccess { config -> brand.value = config.toBrand() }
    }

    private fun PaymentAssetConfig?.toBrand(): PaymentAssetBrand = PaymentAssetBrand(
        symbol = this?.symbol ?: CurrencyConfig.defaultSymbol,
        squareLogo = this?.squareLogoUrl.toLogo(),
        wideLogo = this?.wideLogoUrl.toLogo(),
    )

    private fun String?.toLogo(): PaymentAssetLogo =
        if (this == null) PaymentAssetLogo.Bundled else PaymentAssetLogo.Remote(this)
}
