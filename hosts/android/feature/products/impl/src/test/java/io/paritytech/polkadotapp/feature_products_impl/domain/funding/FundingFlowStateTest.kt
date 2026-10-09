package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingLimit
import uniffi.truapi.FundingMode
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.FundingUnsupported
import uniffi.truapi.RouteDirection
import java.math.BigDecimal

class FundingFlowStateTest {
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
                .receiving(refused("a", FundingQuoteRefusal.BelowMinimum("20000000")))
                .receiving(refused("b", FundingQuoteRefusal.BelowMinimum("10000000")))
        }

        assertEquals(FundingAmountIssue.BelowMinimum(BigDecimal("10")), flow.refusalForCurrentAmount)
        assertFalse(flow.canContinue)
    }

    @Test
    fun `one quote among refusals lets the amount through`() {
        val flow = state(listOf(candidate("a", card()), candidate("b", card())), amount = "5").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(refused("a", FundingQuoteRefusal.BelowMinimum("20000000")))
                .receiving(quoted("b", send = "500"))
        }

        assertNull(flow.amountIssue)
        assertEquals("b", flow.selectedProviderId)
    }

    @Test
    fun `withdrawing more than the balance is refused locally`() {
        val flow = state(listOf(candidate("a", card(RouteDirection.OUT))), FundingDirection.OUT, amount = "300")

        assertEquals(FundingAmountIssue.NotEnoughBalance, flow.amountIssue)
    }

    @Test
    fun `the best quote is the least to pay in and the most received out`() {
        val asked = state(listOf(candidate("a", card()), candidate("b", card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
        }

        assertEquals("b", asked.receiving(quoted("a", send = "5200")).receiving(quoted("b", send = "5100")).bestProviderId)
        assertEquals("a", asked.receiving(quoted("a", send = "5200")).receiving(quoted("b", send = "5100")).withChosenProvider("a").selectedProviderId)
    }

    @Test
    fun `a country is unsupported only when every provider refuses it`() {
        val oneRefuses = state(
            listOf(
                candidate("a", card(), unsupported = listOf(FundingUnsupported(FundingRail.CARD, "EUR", "FR"))),
                candidate("b", card()),
            ),
        )
        val bothRefuse = state(
            listOf(
                candidate("a", card(), unsupported = listOf(FundingUnsupported(FundingRail.CARD, "EUR", "FR"))),
                candidate("b", card(), unsupported = listOf(FundingUnsupported(FundingRail.CARD, "EUR", "fr"))),
            ),
        )

        assertEquals(emptySet<String>(), oneRefuses.unsupportedCountries)
        assertEquals(setOf("FR"), bothRefuse.unsupportedCountries)
    }

    @Test
    fun `every row refusing the asked country marks it unsupported`() {
        val flow = state(listOf(candidate("a", card()), candidate("b", card())), amount = "50").let { start ->
            start.asking(requireNotNull(start.currentAsk()))
                .receiving(refused("a", FundingQuoteRefusal.CountryUnsupported))
                .receiving(refused("b", FundingQuoteRefusal.CountryUnsupported))
        }

        assertEquals(setOf("DE"), flow.unsupportedCountries)
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
        unsupported: List<FundingUnsupported> = emptyList(),
        limits: List<FundingLimit> = emptyList(),
    ) = FundingCandidate(
        providerId = providerId,
        routes = listOf(route),
        unsupported = unsupported,
        limits = limits,
        backend = null,
    )

    private fun card(direction: RouteDirection = RouteDirection.IN, assets: List<String> = listOf("EUR")) =
        route(FundingMode.CARD, direction, assets, networks = null)

    private fun crypto(assets: List<String> = listOf("USDC"), networks: List<String> = listOf("ethereum")) =
        route(FundingMode.CRYPTO, RouteDirection.IN, assets, networks)

    private fun route(mode: FundingMode, direction: RouteDirection, assets: List<String>, networks: List<String>?) =
        FundingRoute(
            mode = mode,
            directions = listOf(direction),
            assets = assets,
            networks = networks,
            countries = null,
            requiresAccount = false,
        )

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
