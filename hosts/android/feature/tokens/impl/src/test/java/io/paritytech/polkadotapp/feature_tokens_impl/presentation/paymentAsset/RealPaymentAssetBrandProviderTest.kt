package io.paritytech.polkadotapp.feature_tokens_impl.presentation.paymentAsset

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetLogo
import io.paritytech.polkadotapp.common.utils.CurrencyConfig
import io.paritytech.polkadotapp.feature_tokens_impl.data.paymentAsset.PaymentAssetConfigProvider
import io.paritytech.polkadotapp.feature_tokens_impl.domain.paymentAsset.PaymentAssetConfig
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

private const val SQUARE_URL = "https://cdn.example.com/square.svg"
private const val WIDE_URL = "https://cdn.example.com/wide.svg"
private const val CACHED_SQUARE_URL = "https://cdn.example.com/cached/square.svg"

class RealPaymentAssetBrandProviderTest {
    private val configProvider: PaymentAssetConfigProvider = mockk()

    private val provider = RealPaymentAssetBrandProvider(configProvider)

    @Test
    fun `should start from the bundled brand with the default symbol`() {
        assertEquals(PaymentAssetBrand.bundled(CurrencyConfig.defaultSymbol), provider.brand.value)
    }

    @Test
    fun `should expose the published symbol and remote logos at once`() = runBlocking<Unit> {
        withNoLastActivatedConfig()
        withPublishedConfig(PaymentAssetConfig("USD", SQUARE_URL, WIDE_URL))

        provider.applyPublishedBrand()

        assertEquals(
            PaymentAssetBrand("USD", PaymentAssetLogo.Remote(SQUARE_URL), PaymentAssetLogo.Remote(WIDE_URL)),
            provider.brand.value
        )
    }

    @Test
    fun `should keep the default symbol when only logos are published`() = runBlocking<Unit> {
        withNoLastActivatedConfig()
        withPublishedConfig(PaymentAssetConfig(null, SQUARE_URL, WIDE_URL))

        provider.applyPublishedBrand()

        assertEquals(
            PaymentAssetBrand(
                symbol = CurrencyConfig.defaultSymbol,
                squareLogo = PaymentAssetLogo.Remote(SQUARE_URL),
                wideLogo = PaymentAssetLogo.Remote(WIDE_URL)
            ),
            provider.brand.value
        )
    }

    @Test
    fun `should keep the bundled logo for a variant that is not published`() = runBlocking<Unit> {
        withNoLastActivatedConfig()
        withPublishedConfig(PaymentAssetConfig("USD", SQUARE_URL, null))

        provider.applyPublishedBrand()

        assertEquals(
            PaymentAssetBrand("USD", PaymentAssetLogo.Remote(SQUARE_URL), PaymentAssetLogo.Bundled),
            provider.brand.value
        )
    }

    @Test
    fun `should keep the bundled brand when the object is not published`() = runBlocking<Unit> {
        withNoLastActivatedConfig()
        withPublishedConfig(null)

        provider.applyPublishedBrand()

        assertEquals(PaymentAssetBrand.bundled(CurrencyConfig.defaultSymbol), provider.brand.value)
    }

    @Test
    fun `should keep the bundled brand when nothing is cached and the config cannot be read`() = runBlocking<Unit> {
        withNoLastActivatedConfig()
        withConfigReadFailure()

        provider.applyPublishedBrand()

        assertEquals(PaymentAssetBrand.bundled(CurrencyConfig.defaultSymbol), provider.brand.value)
    }

    @Test
    fun `should keep the last activated brand when the sync read fails`() = runBlocking<Unit> {
        withLastActivatedConfig(PaymentAssetConfig("USD", CACHED_SQUARE_URL, null))
        withConfigReadFailure()

        provider.applyPublishedBrand()

        assertEquals(
            PaymentAssetBrand("USD", PaymentAssetLogo.Remote(CACHED_SQUARE_URL), PaymentAssetLogo.Bundled),
            provider.brand.value
        )
    }

    @Test
    fun `should replace the last activated brand with the synced one`() = runBlocking<Unit> {
        withLastActivatedConfig(PaymentAssetConfig("USD", CACHED_SQUARE_URL, null))
        withPublishedConfig(PaymentAssetConfig("EUR", SQUARE_URL, WIDE_URL))

        provider.applyPublishedBrand()

        assertEquals(
            PaymentAssetBrand("EUR", PaymentAssetLogo.Remote(SQUARE_URL), PaymentAssetLogo.Remote(WIDE_URL)),
            provider.brand.value
        )
    }

    @Test
    fun `should fall back to the bundled brand when the synced config was unpublished`() = runBlocking<Unit> {
        withLastActivatedConfig(PaymentAssetConfig("USD", CACHED_SQUARE_URL, null))
        withPublishedConfig(null)

        provider.applyPublishedBrand()

        assertEquals(PaymentAssetBrand.bundled(CurrencyConfig.defaultSymbol), provider.brand.value)
    }

    private suspend fun withNoLastActivatedConfig() {
        coEvery { configProvider.lastActivatedPaymentAssetConfig() } returns Result.success(null)
    }

    private suspend fun withLastActivatedConfig(config: PaymentAssetConfig) {
        coEvery { configProvider.lastActivatedPaymentAssetConfig() } returns Result.success(config)
    }

    private suspend fun withPublishedConfig(config: PaymentAssetConfig?) {
        coEvery { configProvider.paymentAssetConfig() } returns Result.success(config)
    }

    private suspend fun withConfigReadFailure() {
        coEvery { configProvider.paymentAssetConfig() } returns Result.failure(IllegalStateException("not synced"))
    }
}
