package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingDirection
import java.math.BigDecimal
import java.util.Locale

class FundingUnitsTest {
    private val cash = FundingCash(symbol = "CASH", precision = 6)

    @Test
    fun `cash units round down so the host never asks for more than typed`() {
        assertEquals("50199999", cash.units(BigDecimal("50.1999999")))
        assertEquals(BigDecimal("50.199999"), cash.decimal("50199999"))
    }

    @Test
    fun `a sub-cent balance rounds down, never up to what cannot be spent`() {
        assertEquals("$9.99", cash.figure(BigDecimal("9.995")))
        assertEquals("$1,000.25 CASH", cash.label(BigDecimal("1000.259")))
    }

    @Test
    fun `cash figures show cents only when there are any`() {
        assertEquals("+$50 CASH", cash.signed(BigDecimal("50"), FundingDirection.IN))
        assertEquals("-$1,226.78 CASH", cash.signed(BigDecimal("1226.78"), FundingDirection.OUT))
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
    fun `an unknown network is shown capitalised`() {
        assertEquals(listOf("Polkadot", "Avalanche"), listOf(FundingNetwork("polkadot"), FundingNetwork("avalanche")).map { it.name })
    }
}
