package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

class FundingUiMapperTest {
    @Test
    fun `arrival times round up to the next unit`() {
        assertEquals(
            listOf(FundingEta.Minutes(1), FundingEta.Minutes(5), FundingEta.Hours(2), FundingEta.Days(2)),
            listOf(30uL, 241uL, 3_700uL, 90_000uL).map(::etaOf),
        )
    }

    @Test
    fun `the core's best of several quotes is marked and the countdown runs to its refresh`() {
        val state = flow()
            .let { it.asking(requireNotNull(it.currentAsk())) }
            .receiving(bothQuoted())

        val providers = state.toProvidersUiState(emptyMap())

        assertEquals(FundingCountdown.Until(Instant.fromEpochMilliseconds(60_000)), providers.countdown)
        assertEquals(listOf(null, FundingProviderBadge.LowestPrice), providers.rows.map { it.badge })
        assertEquals(
            FundingProviderPrice.Quoted(price = "51.00 EUR", forAmount = "$50 CASH"),
            providers.rows[1].price,
        )
    }

    @Test
    fun `the fee breakdown adds up to what the user pays`() {
        val fees = requireNotNull(
            flow()
                .let { it.asking(requireNotNull(it.currentAsk())) }
                .receiving(bothQuoted())
                .toFeesUiState(),
        )

        assertEquals(listOf("50.00 EUR", "0.90 EUR", "0.10 EUR", "1.00 EUR", "51.00 EUR"), with(fees) { listOf(amount, providerFee, networkFee, totalFee, pay) })
    }

    @Test
    fun `asking the same again keeps what each price was for while the new one loads`() {
        val quotedOnce = flow()
            .let { it.asking(requireNotNull(it.currentAsk())) }
            .receiving(bothQuoted())

        val providers = quotedOnce.asking(requireNotNull(quotedOnce.currentAsk()))
            .toProvidersUiState(emptyMap())

        assertEquals(FundingCountdown.Refreshing, providers.countdown)
        assertEquals(FundingProviderPrice.Updating(forAmount = "$50 CASH"), providers.rows[0].price)
    }

    @Test
    fun `the band line follows the rail, and retrying or delay reads as a warning`() {
        val bank = flow().copy(rail = FundingRail.BANK)
        val crypto = flow().copy(rail = FundingRail.CRYPTO)

        assertEquals(FundingBandUi(RCommon.string.funding_band_waiting_bank, null, warning = false), bank.bandLine(FundingBand.AWAITING_PAYMENT, null))
        assertEquals(FundingBandUi(RCommon.string.funding_band_retrying_bank, null, warning = true), bank.bandLine(FundingBand.RETRYING, null))
        assertEquals(FundingBandUi(RCommon.string.funding_band_confirming_deposit, "USDC", warning = false), crypto.bandLine(FundingBand.CHECKING_PAYMENT, "USDC"))
        assertEquals(FundingBandUi(RCommon.string.funding_band_adding, "$50 CASH", warning = false), bank.bandLine(FundingBand.ADDING, null))
        assertEquals(FundingBandUi(RCommon.string.funding_band_delayed, null, warning = true), bank.bandLine(FundingBand.DELAYED, null))
        assertNull(bank.bandLine(FundingBand.ENDED, null))
    }

    private fun flow() = FundingFlowState.initial(
        direction = FundingDirection.IN,
        cash = FundingCash(symbol = "CASH", precision = 6),
        spendable = null,
        candidates = listOf(candidate("a"), candidate("b")),
        amount = null,
        country = FundingCountry("DE", "Germany", "EUR", "Euro"),
    ).withAmountText("50")

    private fun candidate(providerId: String) = FundingCandidate(
        providerId = providerId,
        routes = listOf(
            FundingRoute(
                rail = FundingRail.CARD,
                directions = listOf(FundingDirection.IN),
                assets = listOf("EUR"),
                networks = null,
                countries = null,
            ),
        ),
        unsupported = emptyList(),
        limits = emptyList(),
    )

    private fun bothQuoted() = FundingQuoteList(
        requestId = 1u,
        rows = listOf(quoted("a", send = "5200", expiresAt = 90_000uL), quoted("b", send = "5100", expiresAt = 60_000uL)),
        best = "b",
        refreshAtMs = 60_000uL,
    )

    private fun quoted(providerId: String, send: String, expiresAt: ULong) = FundingQuoteRow(
        providerId = providerId,
        state = FundingQuoteState.Quoted(
            FundingQuote(
                quoteId = "q-$providerId",
                sendAmount = send,
                receiveAmount = "50000000",
                providerFee = "90",
                networkFee = "10",
                etaSecs = null,
                expiresAt = expiresAt,
                assetDecimals = 2u,
            ),
        ),
    )

    /** A fee under a cent still costs something, so it rounds up; a balance never reads as more than it is. */
    @Test
    fun `a fee rounds up to the cent while a balance rounds down`() {
        val cash = FundingCash(symbol = "CASH", precision = 6)

        assertEquals("$0.01 CASH", cash.cost(cash.decimal("9000")))
        assertEquals("$0.00 CASH", cash.label(cash.decimal("9000")))
        assertEquals("$1,000.26 CASH", cash.cost(cash.decimal("1000250001")))
        assertEquals("$1,000.25 CASH", cash.label(cash.decimal("1000259999")))
    }

    @Test
    fun `a cancel offer counts down to its deadline and stops at zero`() {
        val offer = FundingCancelUiState.Offered(fee = "$1 CASH", refund = "$99 CASH", expiresAtMs = 10_000)

        assertEquals(9L, offer.secondsLeft(nowMs = 1_000))
        assertEquals(0L, offer.secondsLeft(nowMs = 12_000))
        assertEquals(null, FundingCancelUiState.Offered(fee = "$1 CASH", refund = "$99 CASH").secondsLeft(nowMs = 0))
    }
}
