package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingCountrySupport
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingNetworkLimits
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import java.math.BigDecimal
import kotlin.time.Instant
import uniffi.truapi.FundingCountry as CoreFundingCountry

// The core decides the rails, the asset, who is asked and the amount rules; these check the screen state reads them.
class FundingFlowStateTest {
    private companion object {
        const val REFRESH_AT_MS = 120_000uL
    }

    private val cash = FundingCash(precision = 6)
    private val germany = FundingCountry(code = "DE", name = "Germany", currencyCode = "EUR", currencyName = "Euro")
    private val providerA = ProductId.parse("a.dot").getOrThrow()
    private val providerB = ProductId.parse("b.dot").getOrThrow()

    @Test
    fun `the core's default rail holds until the user picks one`() {
        val flow = state(options(defaultRail = FundingRail.CRYPTO))
        val picked = flow.withRail(FundingRail.CARD)

        assertEquals(FundingRail.CRYPTO, flow.rail)
        assertEquals(FundingRail.CRYPTO, state(null).withOptions(options(defaultRail = FundingRail.CRYPTO)).rail)
        assertEquals(FundingRail.CARD, picked.withOptions(options(defaultRail = FundingRail.CRYPTO)).rail)
    }

    @Test
    fun `quotes are asked once the core names the asset`() {
        assertEquals("50000000", state(options(asset = "EUR"), amount = "50").askAmount)
        assertNull(state(options(asset = null), amount = "50").askAmount)
        assertNull(state(options(asset = "EUR"), amount = "0").askAmount)
    }

    @Test
    fun `the selection carries the country's currency`() {
        assertEquals(
            FundingSelection(rail = FundingRail.CARD, asset = null, network = null, country = "DE", currency = "EUR"),
            state(options(asset = "EUR")).selection,
        )
    }

    @Test
    fun `text that is no amount asks for nothing`() {
        val flow = state(options(asset = "EUR"), amount = "5.")

        assertEquals("5000000", flow.units)
        assertNull(state(options(asset = "EUR"), amount = "abc").units)
        assertNull(state(options(asset = "EUR"), amount = "abc").askAmount)
    }

    @Test
    fun `the core's amount issue holds back continuing`() {
        val issue = FundingAmountIssue.BelowMinimum("10000000")

        assertFalse(state(options(), amount = "5").withAmountIssue(issue).canContinue)
        assertTrue(state(options(), amount = "50").withAmountIssue(null).canContinue)
    }

    @Test
    fun `quoting lasts until every provider on the list answered`() {
        val asked = state(options(asset = "EUR"), amount = "50").asked()
        val pending = asked.receiving(list(FundingQuoteRow(providerA, FundingQuoteState.Pending), complete = false))

        assertTrue(asked.isQuoting)
        assertTrue(pending.isQuoting)
        assertFalse(asked.receiving(list(quoted(providerA, send = "500"), best = providerA)).isQuoting)
    }

    @Test
    fun `a list the core withdrew drops the quotes`() {
        val quoted = state(options(asset = "EUR"), amount = "50").asked()
            .receiving(list(quoted(providerA, send = "500"), best = providerA))

        val withdrawn = quoted.receiving(null)

        assertNull(withdrawn.list)
        assertNull(withdrawn.selectedProviderId)
        assertNull(withdrawn.askedAmount)
    }

    @Test
    fun `the refusals' amount issue explains a list no provider quoted`() {
        val issue = FundingAmountIssue.AboveMaximum("1000")
        val refused = state(options(asset = "EUR"), amount = "50").asked()
            .receiving(list(FundingQuoteRow(providerA, FundingQuoteState.Unavailable(FundingQuoteUnavailable.Timeout)), issue = issue))

        assertTrue(refused.noProviderQuoted)
        assertEquals(issue, refused.unavailableIssue)
    }

    @Test
    fun `the core's best quote is selected until the user picks another`() {
        val quoted = state(options(asset = "EUR"), amount = "50").let { start ->
            start.asked()
                .receiving(list(quoted(providerA, send = "5200"), quoted(providerB, send = "5100"), best = providerB))
        }

        assertEquals(listOf(providerB, providerA), listOf(quoted.selectedProviderId, quoted.withChosenProvider(providerA).selectedProviderId))
        assertEquals(Instant.fromEpochMilliseconds(REFRESH_AT_MS.toLong()), quoted.refreshAt)
        assertEquals(listOf(providerA, providerB), quoted.servingCandidates.map { it.providerId })
    }

    // Asking the same amount again starts every row pending; the old price stays on screen, greyed, until the new one.
    @Test
    fun `a row pending after the same amount again keeps its last quote greyed`() {
        val quote = quoted(providerA, send = "5200")
        val refreshed = state(options(asset = "EUR"), amount = "50").asked()
            .receiving(list(quote, best = providerA))
            .asked()
            .receiving(list(FundingQuoteRow(providerA, FundingQuoteState.Pending), requestId = 2u, complete = false))

        assertEquals(mapOf(providerA to (quote.state as FundingQuoteState.Quoted).quote), refreshed.staleQuotes)
        assertNull(refreshed.selectedProviderId)
    }

    @Test
    fun `the core's country support marks unsupported countries and lists each country's minimum`() {
        val flow = state(
            options(
                countries = listOf(
                    CoreFundingCountry("fr", FundingCountrySupport.Unsupported),
                    CoreFundingCountry("DE", FundingCountrySupport.Supported(min = "20000000")),
                    CoreFundingCountry("PT", FundingCountrySupport.Supported(min = null)),
                ),
            ),
        )

        assertEquals(setOf("FR"), flow.unsupportedCountries)
        assertEquals(mapOf("DE" to BigDecimal("20")), flow.countryMinimums)
    }

    @Test
    fun `networks carry the core's minimums`() {
        val flow = state(
            options(
                defaultRail = FundingRail.CRYPTO,
                networks = listOf(
                    FundingNetworkLimits("ethereum", FundingAmountLimits(min = "30000000", max = null)),
                    FundingNetworkLimits("polkadot", FundingAmountLimits(min = null, max = null)),
                ),
            ),
        )

        assertEquals(listOf("ethereum", "polkadot"), flow.networks)
        assertEquals(BigDecimal("30") to null, flow.minimum("ethereum") to flow.minimum("polkadot"))
    }

    @Test
    fun `changing the rail drops the network, token and quotes`() {
        val start = state(options(asset = "EUR"), amount = "50")
        val flow = start.asked().withRail(FundingRail.CRYPTO).withNetwork("ethereum")

        assertEquals(start.copy(rail = FundingRail.CRYPTO, railChosen = true, network = "ethereum"), flow)
        assertNull(flow.withRail(FundingRail.CARD).network)
    }

    /** Asks for quotes of the amount on screen. */
    private fun FundingFlowState.asked(): FundingFlowState = asking(requireNotNull(askAmount))

    private fun state(
        options: FundingOptions?,
        direction: FundingDirection = FundingDirection.IN,
        amount: String = "",
    ) = FundingFlowState.initial(
        direction = direction,
        cash = cash,
        format = testCashFormat(),
        spendable = BigDecimal("226.78"),
        options = options,
        amount = null,
        country = germany,
    ).withAmountText(amount)

    private fun options(
        defaultRail: FundingRail = FundingRail.CARD,
        asset: String? = "EUR",
        limits: FundingAmountLimits = FundingAmountLimits(min = null, max = null),
        countries: List<CoreFundingCountry> = emptyList(),
        networks: List<FundingNetworkLimits> = emptyList(),
    ) = FundingOptions(
        candidates = listOf(candidate(providerA), candidate(providerB)),
        countries = countries,
        limits = limits,
        networks = networks,
        rails = listOf(defaultRail) + (listOf(FundingRail.CRYPTO, FundingRail.CARD) - defaultRail),
        asset = asset,
        tokens = emptyList(),
        detectedCountry = null,
    )

    private fun candidate(providerId: ProductId) = FundingCandidate(
        providerId = providerId,
        routes = emptyList(),
        unsupported = emptyList(),
        limits = emptyList(),
        declaresRoutes = true,
        brand = null,
    )

    private fun list(
        vararg rows: FundingQuoteRow,
        best: ProductId? = null,
        requestId: ULong = 1u,
        complete: Boolean = true,
        issue: FundingAmountIssue? = null,
    ) = FundingQuoteList(
        requestId = requestId,
        rows = rows.toList(),
        best = best,
        refreshAtMs = REFRESH_AT_MS,
        amount = "50000000",
        refusal = null,
        issue = issue,
        complete = complete,
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
