package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingNetworkLimits
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.ProductId
import java.util.Locale
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

class FundingUiMapperTest {
    private val providerA = ProductId.parse("a.dot").getOrThrow()
    private val providerB = ProductId.parse("b.dot").getOrThrow()

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
            .asked()
            .received(bothQuoted())

        val providers = state.toProvidersUiState(emptyMap(), Instant.fromEpochMilliseconds(0))

        assertEquals(FundingCountdown.Until(Instant.fromEpochMilliseconds(60_000)), providers.countdown)
        assertEquals(listOf(null, FundingProviderBadge.LowestPrice), providers.rows.map { it.badge })
        assertEquals(
            FundingProviderPrice.Quoted(price = "51.00 EUR", forAmount = "$50 CASH"),
            providers.rows[1].price,
        )
    }

    @Test
    fun `the fee breakdown adds up to what the user pays`() {
        val previous = Locale.getDefault()
        Locale.setDefault(Locale.US)
        try {
            val fees = requireNotNull(
                flow()
                    .asked()
                    .received(bothQuoted())
                    .toFeesUiState(),
            )

            assertEquals(
                listOf("€50.00", "€0.90", "€0.10", "€1.00", "€51.00"),
                with(fees) { listOf(amount, providerFee, networkFee, totalFee, pay) },
            )
        } finally {
            Locale.setDefault(previous)
        }
    }

    @Test
    fun `asking the same again keeps what each price was for while the new one loads`() {
        val quotedOnce = flow()
            .asked()
            .received(bothQuoted())

        val providers = quotedOnce.asked()
            .toProvidersUiState(emptyMap(), Instant.fromEpochMilliseconds(0))

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

    @Test
    fun `a picker draws each network and token with the first listed provider whose routes offer it`() {
        val crypto = cryptoFlow(
            cryptoCandidate(providerA, networks = listOf("polkadot"), assets = listOf("USDC")),
            cryptoCandidate(providerB, networks = listOf("polkadot", "ethereum"), assets = listOf("USDC", "USDT")),
        )

        assertEquals(
            listOf("funding-icon:a.dot/networks/polkadot", "funding-icon:b.dot/networks/ethereum", null),
            crypto.toNetworkRows().map { it.icon.image },
        )
        assertEquals(
            listOf("funding-icon:b.dot/tokens/usdc", "funding-icon:b.dot/tokens/usdt"),
            crypto.withNetwork("ethereum").toTokensUiState().rows.map { it.icon.image },
        )
        assertEquals("funding-icon:a.dot/tokens/usdc", crypto.withNetwork("polkadot").toTokensUiState().rows.first().icon.image)
    }

    @Test
    fun `a network or token no provider offers keeps its first letter`() {
        val crypto = cryptoFlow(cryptoCandidate(providerA, networks = listOf("polkadot"), assets = listOf("USDC")))
        val base = crypto.toNetworkRows().last()

        assertEquals(FundingCryptoIcon(monogram = "B", image = null), base.icon)
    }

    /** Asks for quotes of the amount on screen, as the funding service reports a new ask of the same amount. */
    private fun FundingFlowState.asked(): FundingFlowState = withQuotes(null, askAmount, failed = false)

    /** The ask's list, as the funding service reports it. */
    private fun FundingFlowState.received(list: FundingQuoteList?): FundingFlowState = withQuotes(list, list?.let { askedAmount ?: it.amount.toBalanceOrNull() }, failed = false)

    private fun flow() = FundingFlowState.initial(
        direction = FundingDirection.IN,
        cashPrecision = 6,
        format = testCashFormat(),
        spendable = null,
        options = FundingOptions(
            candidates = listOf(candidate(providerA), candidate(providerB)),
            countries = emptyList(),
            limits = FundingAmountLimits(min = null, max = null),
            networks = emptyList(),
            rails = listOf(FundingRail.CARD),
            asset = "EUR",
            tokens = emptyList(),
            detectedCountry = null,
        ),
        amount = null,
        country = FundingCountry("DE", "Germany", "EUR", "Euro"),
    ).withAmountText("50")

    private fun cryptoFlow(vararg candidates: FundingCandidate) = FundingFlowState.initial(
        direction = FundingDirection.IN,
        cashPrecision = 6,
        format = testCashFormat(),
        spendable = null,
        options = FundingOptions(
            candidates = candidates.toList(),
            countries = emptyList(),
            limits = FundingAmountLimits(min = null, max = null),
            networks = listOf("polkadot", "ethereum", "base").map { FundingNetworkLimits(it, FundingAmountLimits(min = null, max = null)) },
            rails = listOf(FundingRail.CRYPTO),
            asset = null,
            tokens = listOf("USDC", "USDT"),
            detectedCountry = null,
        ),
        amount = null,
        country = null,
    )

    private fun cryptoCandidate(providerId: ProductId, networks: List<String>, assets: List<String>) = FundingCandidate(
        providerId = providerId,
        routes = listOf(
            FundingRoute(
                rail = FundingRail.CRYPTO,
                directions = listOf(FundingDirection.IN),
                assets = assets,
                networks = networks,
                countries = null,
            ),
        ),
        unsupported = emptyList(),
        limits = emptyList(),
        declaresRoutes = true,
        brand = null,
    )

    private fun candidate(providerId: ProductId) = FundingCandidate(
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
        declaresRoutes = true,
        brand = null,
    )

    private fun bothQuoted() = FundingQuoteList(
        requestId = 1u,
        rows = listOf(quoted(providerA, send = "5200", expiresAt = 90_000uL), quoted(providerB, send = "5100", expiresAt = 60_000uL)),
        best = providerB,
        refreshAtMs = 60_000uL,
        amount = "50000000",
        refusal = null,
        issue = null,
        complete = true,
    )

    private fun quoted(providerId: ProductId, send: String, expiresAt: ULong) = FundingQuoteRow(
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

    /** A fee under a cent still costs something, so it rounds up. A balance rounding down is in FundingUnitsTest. */
    @Test
    fun `a fee rounds up to the cent`() {
        assertEquals("$0.01 CASH", testCashFormat().cost(flow().cashAmount(9000.intoBalance())))
        assertEquals("$1,000.26 CASH", testCashFormat().cost(flow().cashAmount(1_000_250_001.intoBalance())))
    }

    @Test
    fun `a cancel offer counts down to its deadline and stops at zero`() {
        val offer = FundingCancelUiState.Offered(fee = "$1 CASH", refund = "$99 CASH", expiresAt = Instant.fromEpochMilliseconds(10_000))

        assertEquals(9L, offer.secondsLeft(now = Instant.fromEpochMilliseconds(1_000)))
        assertEquals(0L, offer.secondsLeft(now = Instant.fromEpochMilliseconds(12_000)))
        val open = FundingCancelUiState.Offered(fee = "$1 CASH", refund = "$99 CASH", expiresAt = null)
        assertEquals(null, open.secondsLeft(now = Instant.fromEpochMilliseconds(0)))
    }
}
