package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountrySupport
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingLimit
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import java.math.BigDecimal
import kotlin.time.Instant
import uniffi.truapi.FundingCountry as CoreFundingCountry

class FundingFlowStateTest {
    private companion object {
        const val REFRESH_AT_MS = 120_000uL
    }

    private val cash = FundingCash(symbol = "CASH", precision = 6)
    private val germany = FundingCountry(code = "DE", name = "Germany", currencyCode = "EUR", currencyName = "Euro")

    @Test
    fun `card is the default rail whenever a provider serves it`() {
        assertEquals(FundingRail.CARD, state(listOf(candidate("a", crypto()), candidate("b", card()))).rail)
        assertEquals(FundingRail.CRYPTO, state(listOf(candidate("a", crypto()))).rail)
    }

    @Test
    fun `the ask names the payment country's currency when a provider takes it`() {
        val ask = state(listOf(candidate("a", card(assets = listOf("EUR", "USD")))), amount = "50").currentAsk()

        assertEquals(
            FundingQuoteAsk(
                direction = FundingDirection.IN,
                rail = FundingRail.CARD,
                asset = "EUR",
                network = null,
                amount = "50000000",
                country = "DE",
            ),
            ask,
        )
    }

    @Test
    fun `no limit shows until a provider has refused`() {
        assertEquals(FundingLimits(null, null), state(listOf(candidate("a", card()))).learnedLimits)
    }

    @Test
    fun `the learned limits span the providers serving the rail`() {
        val flow = state(
            listOf(
                candidate("a", card(), limits = listOf(limit(FundingRail.CARD, min = "10000000", max = "2000000000"))),
                candidate("b", card(), limits = listOf(limit(FundingRail.CARD, min = "20000000", max = "5000000000"))),
                candidate("c", crypto(), limits = listOf(limit(FundingRail.CRYPTO, min = "1000000", max = null))),
            ),
            amount = "5",
        )

        assertEquals(FundingLimits(BigDecimal("10"), BigDecimal("5000")), flow.learnedLimits)
        assertEquals(FundingAmountIssue.BelowMinimum(BigDecimal("10")), flow.amountIssue)
    }

    @Test
    fun `an amount every provider refused reads as the lowest minimum`() {
        val flow = state(listOf(candidate("a", card()), candidate("b", card())), amount = "5").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(list(refused("a", FundingQuoteRefusal.BelowMinimum("20000000")), refused("b", FundingQuoteRefusal.BelowMinimum("10000000"))))
        }

        assertEquals(FundingAmountIssue.BelowMinimum(BigDecimal("10")), flow.refusalForCurrentAmount)
        assertFalse(flow.canContinue)
    }

    @Test
    fun `one quote among refusals lets the amount through`() {
        val flow = state(listOf(candidate("a", card()), candidate("b", card())), amount = "5").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(list(refused("a", FundingQuoteRefusal.BelowMinimum("20000000")), quoted("b", send = "500"), best = "b"))
        }

        assertNull(flow.amountIssue)
        assertEquals("b", flow.selectedProviderId)
    }

    @Test
    fun `withdrawing more than the balance is refused locally`() {
        val flow = state(listOf(candidate("a", card(FundingDirection.OUT))), FundingDirection.OUT, amount = "300")

        assertEquals(FundingAmountIssue.NotEnoughBalance, flow.amountIssue)
    }

    @Test
    fun `the core's best quote is selected until the user picks another`() {
        val quoted = state(listOf(candidate("a", card()), candidate("b", card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(list(quoted("a", send = "5200"), quoted("b", send = "5100"), best = "b"))
        }

        assertEquals(listOf("b", "a"), listOf(quoted.selectedProviderId, quoted.withChosenProvider("a").selectedProviderId))
        assertEquals(Instant.fromEpochMilliseconds(REFRESH_AT_MS.toLong()), quoted.refreshAt)
    }

    // At the core's refresh every row goes back to pending; the old price stays on screen, greyed, until the new one.
    @Test
    fun `a row back to pending at a refresh keeps its last quote greyed`() {
        val quote = quoted("a", send = "5200")
        val refreshed = state(listOf(candidate("a", card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(list(quote, best = "a"))
                .receiving(list(FundingQuoteRow("a", FundingQuoteState.Pending)))
        }

        assertEquals(mapOf("a" to (quote.state as FundingQuoteState.Quoted).quote), refreshed.staleQuotes)
        assertNull(refreshed.selectedProviderId)
    }

    @Test
    fun `the core's country support marks unsupported countries and asks for the payment country's minimum`() {
        val flow = state(listOf(candidate("a", card())), amount = "5").withCountrySupport(
            listOf(
                CoreFundingCountry("fr", FundingCountrySupport.Unsupported),
                CoreFundingCountry("DE", FundingCountrySupport.Supported(min = "20000000")),
                CoreFundingCountry("PT", FundingCountrySupport.Supported(min = null)),
            ),
        )

        assertEquals(setOf("FR"), flow.unsupportedCountries)
        assertEquals(mapOf("DE" to BigDecimal("20")), flow.countryMinimums)
        assertEquals(FundingAmountIssue.BelowMinimum(BigDecimal("20")), flow.amountIssue)
    }

    @Test
    fun `networks and tokens are the union of the crypto routes`() {
        val flow = state(
            listOf(
                candidate("a", crypto(assets = listOf("USDC"), networks = listOf("ethereum", "polkadot"))),
                candidate("b", crypto(assets = listOf("USDT"), networks = listOf("ethereum", "tron"))),
            ),
        )

        assertEquals(listOf("ethereum", "polkadot", "tron"), flow.networks.map { it.id })
        assertEquals(listOf("USDT"), flow.withNetwork("tron").tokens.map { it.symbol })
    }

    @Test
    fun `changing the rail drops the network, token and quotes`() {
        val flow = state(listOf(candidate("a", card()), candidate("b", crypto())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk())).withRail(FundingRail.CRYPTO).withNetwork("ethereum")
        }

        assertEquals(state(listOf(candidate("a", card()), candidate("b", crypto())), amount = "50").copy(rail = FundingRail.CRYPTO, network = "ethereum"), flow)
        assertNull(flow.withRail(FundingRail.CARD).network)
    }

    private fun state(
        candidates: List<FundingCandidate>,
        direction: FundingDirection = FundingDirection.IN,
        amount: String = "",
    ) = FundingFlowState.initial(
        direction = direction,
        cash = cash,
        spendable = BigDecimal("226.78"),
        candidates = candidates,
        amount = null,
        country = germany,
    ).withAmountText(amount)

    private fun candidate(
        providerId: String,
        route: FundingRoute,
        limits: List<FundingLimit> = emptyList(),
    ) = FundingCandidate(
        providerId = providerId,
        routes = listOf(route),
        unsupported = emptyList(),
        limits = limits,
    )

    private fun card(direction: FundingDirection = FundingDirection.IN, assets: List<String> = listOf("EUR")) =
        route(FundingRail.CARD, direction, assets, networks = null)

    private fun crypto(assets: List<String> = listOf("USDC"), networks: List<String> = listOf("ethereum")) =
        route(FundingRail.CRYPTO, FundingDirection.IN, assets, networks)

    private fun route(rail: FundingRail, direction: FundingDirection, assets: List<String>, networks: List<String>?) =
        FundingRoute(
            rail = rail,
            directions = listOf(direction),
            assets = assets,
            networks = networks,
            countries = null,
        )

    private fun list(vararg rows: FundingQuoteRow, best: String? = null) =
        FundingQuoteList(rows = rows.toList(), best = best, refreshAtMs = REFRESH_AT_MS)

    private fun limit(rail: FundingRail, min: String?, max: String?) =
        FundingLimit(rail = rail, asset = "EUR", network = null, min = min, max = max)

    private fun refused(providerId: String, refusal: FundingQuoteRefusal) = FundingQuoteRow(
        providerId = providerId,
        state = FundingQuoteState.Unavailable(FundingQuoteUnavailable.Refused(refusal)),
    )

    private fun quoted(providerId: String, send: String) = FundingQuoteRow(
        providerId = providerId,
        state = FundingQuoteState.Quoted(
            FundingQuote(
                quoteId = "q-$providerId",
                sendAmount = send,
                receiveAmount = "50000000",
                providerFee = "100",
                networkFee = "5",
                etaSecs = 300uL,
                expiresAt = 60_000uL,
            ),
        ),
    )
}
