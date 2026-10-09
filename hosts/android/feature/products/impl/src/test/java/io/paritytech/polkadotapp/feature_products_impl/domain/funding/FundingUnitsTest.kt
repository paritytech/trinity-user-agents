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
    fun `cash figures show cents only when there are any`() {
        assertEquals("+$50 CASH", cash.signed(BigDecimal("50"), FundingDirection.IN))
        assertEquals("-$1,226.78 CASH", cash.signed(BigDecimal("1226.78"), FundingDirection.OUT))
    }

    @Test
    fun `fiat minor units come from ISO 4217`() {
        assertEquals(FundingAssetUnit("EUR", 2, isFiat = true), FundingAssetUnit.of("eur"))
        assertEquals(FundingAssetUnit("JPY", 0, isFiat = true), FundingAssetUnit.of("JPY"))
        assertEquals(FundingAssetUnit("KWD", 3, isFiat = true), FundingAssetUnit.of("KWD"))
    }

    @Test
    fun `crypto decimals come from the table, and an unknown symbol reads as six`() {
        assertEquals(
            listOf(6, 6, 10, 18, 8, 9, 6, 6),
            listOf("USDC", "USDT", "DOT", "ETH", "BTC", "SOL", "TRX", "XYZ").map { FundingAssetUnit.of(it).decimals },
        )
    }

    @Test
    fun `quotes are set as the provider list and deposit screens show them`() {
        assertEquals("25.10 EUR", FundingAssetUnit.of("EUR").formatWithCode("2510"))
        assertEquals("51.8 USDT", FundingAssetUnit.of("USDT").format("51800000", Locale.US))
        assertEquals("€50.05", FundingAssetUnit.of("EUR").format("5005", Locale.US))
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
