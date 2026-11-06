package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountrySupport
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingLimit
import uniffi.truapi.FundingNetworkLimits
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.ProductId
import java.math.BigDecimal
import kotlin.time.Instant
import uniffi.truapi.FundingCountry as CoreFundingCountry

class FundingFlowStateTest {
    private companion object {
        const val REFRESH_AT_MS = 120_000uL
    }

    private val cash = FundingCash(symbol = "CASH", precision = 6)
    private val germany = FundingCountry(code = "DE", name = "Germany", currencyCode = "EUR", currencyName = "Euro")
    private val providerA = ProductId.parse("a.dot").getOrThrow()
    private val providerB = ProductId.parse("b.dot").getOrThrow()

    @Test
    fun `card is the default rail whenever a provider serves it`() {
        assertEquals(FundingRail.CARD, state(listOf(candidate(providerA, crypto()), candidate(providerB, card()))).rail)
        assertEquals(FundingRail.CRYPTO, state(listOf(candidate(providerA, crypto()))).rail)
    }

    @Test
    fun `the ask names the payment country's currency when a provider takes it`() {
        val ask = state(listOf(candidate(providerA, card(assets = listOf("EUR", "USD")))), amount = "50").currentAsk()

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
    fun `no limit shows until the core reports one`() {
        val flow = state(listOf(candidate(providerA, card())), amount = "5")

        assertEquals(FundingLimits(null, null), flow.amountLimits)
        assertNull(flow.amountIssue)
    }

    @Test
    fun `the core's limits bound the amount`() {
        val limits = FundingLimits(BigDecimal("10"), BigDecimal("5000"))
        val below = state(listOf(candidate(providerA, card())), amount = "5").withAmountLimits(limits)
        val above = state(listOf(candidate(providerA, card())), amount = "6000").withAmountLimits(limits)

        assertEquals(FundingAmountIssue.BelowMinimum(BigDecimal("10")), below.amountIssue)
        assertFalse(below.canContinue)
        assertEquals(FundingAmountIssue.AboveMaximum(BigDecimal("5000")), above.amountIssue)
    }

    @Test
    fun `a list for an earlier request is dropped`() {
        val asked = state(listOf(candidate(providerA, card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk())).requested(2u)
        }

        val late = asked.receiving(list(quoted(providerA, send = "500"), best = providerA, requestId = 1u))

        assertSame(asked, late)
        assertNull(late.selectedProviderId)
    }

    @Test
    fun `a new ask drops the lists of the request before it`() {
        val first = state(listOf(candidate(providerA, card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .requested(1u)
                .receiving(list(quoted(providerA, send = "500"), best = providerA, requestId = 1u))
        }
        val second = first.withAmountText("60").let { it.asking(requireNotNull(it.currentAsk())) }

        assertSame(second, second.receiving(list(quoted(providerA, send = "500"), best = providerA, requestId = 1u)))
        assertEquals(providerA, second.receiving(list(quoted(providerA, send = "600"), best = providerA, requestId = 2u)).selectedProviderId)
    }

    @Test
    fun `one quote among refusals lets the amount through`() {
        val flow = state(listOf(candidate(providerA, card()), candidate(providerB, card())), amount = "5").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(list(refused(providerA, FundingQuoteRefusal.BelowMinimum("20000000")), quoted(providerB, send = "500"), best = providerB))
        }

        assertNull(flow.amountIssue)
        assertEquals(providerB, flow.selectedProviderId)
    }

    @Test
    fun `withdrawing more than the balance is refused locally`() {
        val flow = state(listOf(candidate(providerA, card(FundingDirection.OUT))), FundingDirection.OUT, amount = "300")

        assertEquals(FundingAmountIssue.NotEnoughBalance, flow.amountIssue)
    }

    @Test
    fun `the core's best quote is selected until the user picks another`() {
        val quoted = state(listOf(candidate(providerA, card()), candidate(providerB, card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(list(quoted(providerA, send = "5200"), quoted(providerB, send = "5100"), best = providerB))
        }

        assertEquals(listOf(providerB, providerA), listOf(quoted.selectedProviderId, quoted.withChosenProvider(providerA).selectedProviderId))
        assertEquals(Instant.fromEpochMilliseconds(REFRESH_AT_MS.toLong()), quoted.refreshAt)
    }

    // Asking the same ask again starts every row pending; the old price stays on screen, greyed, until the new one.
    @Test
    fun `a row pending after the same ask again keeps its last quote greyed`() {
        val quote = quoted(providerA, send = "5200")
        val refreshed = state(listOf(candidate(providerA, card())), amount = "50").let { start ->
            val ask = requireNotNull(start.currentAsk())
            start.asking(ask)
                .receiving(list(quote, best = providerA))
                .asking(ask)
                .receiving(list(FundingQuoteRow(providerA, FundingQuoteState.Pending), requestId = 2u))
        }

        assertEquals(mapOf(providerA to (quote.state as FundingQuoteState.Quoted).quote), refreshed.staleQuotes)
        assertNull(refreshed.selectedProviderId)
    }

    @Test
    fun `the core's country support marks unsupported countries and lists each country's minimum`() {
        val flow = state(listOf(candidate(providerA, card())), amount = "5").withCountrySupport(
            listOf(
                CoreFundingCountry("fr", FundingCountrySupport.Unsupported),
                CoreFundingCountry("DE", FundingCountrySupport.Supported(min = "20000000")),
                CoreFundingCountry("PT", FundingCountrySupport.Supported(min = null)),
            ),
        )

        assertEquals(setOf("FR"), flow.unsupportedCountries)
        assertEquals(mapOf("DE" to BigDecimal("20")), flow.countryMinimums)
    }

    @Test
    fun `networks and tokens are the union of the crypto routes`() {
        val flow = state(
            listOf(
                candidate(providerA, crypto(assets = listOf("USDC"), networks = listOf("ethereum", "polkadot"))),
                candidate(providerB, crypto(assets = listOf("USDT"), networks = listOf("ethereum", "tron"))),
            ),
        )

        assertEquals(listOf("ethereum", "polkadot", "tron"), flow.networks.map { it.id })
        assertEquals(listOf("USDT"), flow.withNetwork("tron").tokens.map { it.symbol })
    }

    @Test
    fun `the core's options set each network's minimum`() {
        val flow = state(listOf(candidate(providerA, crypto(networks = listOf("ethereum", "polkadot"))))).withOptions(
            FundingOptions(
                candidates = listOf(candidate(providerA, crypto(networks = listOf("ethereum", "polkadot")))),
                countries = emptyList(),
                limits = FundingAmountLimits(min = null, max = null),
                networks = listOf(
                    FundingNetworkLimits("ethereum", FundingAmountLimits(min = "30000000", max = null)),
                    FundingNetworkLimits("polkadot", FundingAmountLimits(min = null, max = null)),
                ),
            ),
        )

        assertEquals(BigDecimal("30") to null, flow.minimum("ethereum") to flow.minimum("polkadot"))
    }

    @Test
    fun `changing the rail drops the network, token and quotes`() {
        val flow = state(listOf(candidate(providerA, card()), candidate(providerB, crypto())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk())).withRail(FundingRail.CRYPTO).withNetwork("ethereum")
        }

        assertEquals(state(listOf(candidate(providerA, card()), candidate(providerB, crypto())), amount = "50").copy(rail = FundingRail.CRYPTO, network = "ethereum"), flow)
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
        providerId: ProductId,
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

    private fun list(vararg rows: FundingQuoteRow, best: ProductId? = null, requestId: ULong = 1u) =
        FundingQuoteList(requestId = requestId, rows = rows.toList(), best = best, refreshAtMs = REFRESH_AT_MS)

    private fun refused(providerId: ProductId, refusal: FundingQuoteRefusal) = FundingQuoteRow(
        providerId = providerId,
        state = FundingQuoteState.Unavailable(FundingQuoteUnavailable.Refused(refusal)),
    )

    private fun quoted(providerId: ProductId, send: String) = FundingQuoteRow(
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
                assetDecimals = 2u,
            ),
        ),
    )
}
