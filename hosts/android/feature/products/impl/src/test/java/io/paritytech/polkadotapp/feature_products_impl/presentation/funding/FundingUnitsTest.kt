package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigDecimal
import java.text.Collator
import java.util.Locale
import io.paritytech.polkadotapp.common.R as RCommon

class FundingUnitsTest {
    private val cash = FundingCash(precision = 6)

    @Test
    fun `cash units read as the balance's decimals`() {
        assertEquals(BigDecimal("50.199999"), cash.decimal("50199999"))
    }

    @Test
    fun `the stated decimals count, and a code ISO 4217 knows reads as money`() {
        assertEquals(FundingAssetUnit("EUR", 2, isFiat = true), FundingAssetUnit.of("eur", 2))
        assertEquals(FundingAssetUnit("ETH", 18, isFiat = false), FundingAssetUnit.of("ETH", 18))
        assertEquals(BigDecimal("1.5"), FundingAssetUnit.of("WETH", 18).decimal("1500000000000000000"))
    }

    @Test
    fun `quotes are set as the provider list and deposit screens show them`() {
        assertEquals("25.10 EUR", FundingAssetUnit.of("EUR", 2).formatWithCode("2510"))
        assertEquals("51.8 USDT", FundingAssetUnit.of("USDT", 6).format("51800000", Locale.US))
        assertEquals("€50.05", FundingAssetUnit.of("EUR", 2).format("5005", Locale.US))
    }

    @Test
    fun `a country carries its currency and flag`() {
        assertEquals(
            FundingCountry(code = "DE", name = "Germany", currencyCode = "EUR", currencyName = "Euro"),
            FundingCountries.country("de", Locale.US),
        )
        assertEquals("🇩🇪", FundingCountries.country("DE", Locale.US)?.flag)
    }

    @Test
    fun `only an ISO 3166 alpha-2 code is a country`() {
        assertNull(FundingCountries.country("DEU", Locale.US))
        assertNull(FundingCountries.country("1A", Locale.US))
    }

    @Test
    fun `countries are listed by name in the user's language`() {
        val names = FundingCountries.all(Locale.US).map { it.name }

        assertTrue("Germany" in names)
        assertEquals(names.sortedWith(Collator.getInstance(Locale.US)), names)
    }

    @Test
    fun `a network's native token shares its logo`() {
        assertEquals(fundingNetworkIcon("bitcoin"), fundingTokenIcon("BTC"))
        assertEquals(fundingNetworkIcon("polkadot"), fundingTokenIcon("dot"))
        assertEquals(FundingCryptoIcon.Drawable(RCommon.drawable.img_crypto_tron), fundingTokenIcon("trx"))
    }

    @Test
    fun `a network or token without a logo shows its first letter`() {
        assertEquals(FundingCryptoIcon.Monogram("B"), fundingNetworkIcon("base"))
        assertEquals(FundingCryptoIcon.Monogram("W"), fundingTokenIcon("wbtc"))
    }
}
