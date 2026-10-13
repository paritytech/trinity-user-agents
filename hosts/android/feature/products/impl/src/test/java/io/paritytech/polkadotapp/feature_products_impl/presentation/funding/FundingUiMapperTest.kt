package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import kotlin.time.Instant

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

        val providers = state.toProvidersUiState(emptyMap(), now = Instant.fromEpochMilliseconds(30_000))

        assertEquals(FundingCountdown.Remaining(30), providers.countdown)
        assertEquals(listOf(null, FundingProviderBadge.LowestPrice), providers.rows.map { it.badge })
        assertEquals(
            FundingProviderPrice.Quoted(price = "51.00 EUR", forAmount = "$50 CASH"),
            providers.rows[1].price,
        )
    }

    @Test
    fun `asking the same again keeps what each price was for while the new one loads`() {
        val quotedOnce = flow()
            .let { it.asking(requireNotNull(it.currentAsk())) }
            .receiving(bothQuoted())

        val providers = quotedOnce.asking(requireNotNull(quotedOnce.currentAsk()))
            .toProvidersUiState(emptyMap(), now = Instant.fromEpochMilliseconds(60_000))

        assertEquals(FundingCountdown.Remaining(0), providers.countdown)
        assertEquals(FundingProviderPrice.Updating(forAmount = "$50 CASH"), providers.rows[0].price)
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
            ),
        ),
    )
}
