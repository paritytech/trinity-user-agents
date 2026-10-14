package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.params
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyCoins
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyVouchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import java.math.BigInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

private const val FULL_RING = 767

class CoinagePreClassificationTest {
    private val minPrivacy = ParametricRecyclingStrategy(RecyclingStrategyType.MIN_PRIVACY.params, forcedAgeOf(FORCED_AGE))
    private val maxPrivacy = ParametricRecyclingStrategy(RecyclingStrategyType.MAX_PRIVACY.params, forcedAgeOf(FORCED_AGE))

    @Test
    fun `a settled coin is minted`() {
        val coin = coinOf(age = Coin.Age.Known(3), onChain = true)

        val buckets = listOf(tracked(coin)).preClassifyCoins()

        assertEquals(listOf(coin), buckets.minted)
        assertEquals(emptyList<Coin>(), buckets.minting)
    }

    /**
     * The guard the strategies lean on: gating needs an age, and a coin whose age nothing has read yet must
     * never be picked for recycling. Asserted on the bucket rather than on the sentinel that produces it.
     */
    @Test
    fun `an on-chain coin whose age is unknown is never minted`() {
        val coin = coinOf(age = Coin.Age.Unknown, onChain = true)

        val buckets = listOf(tracked(coin, state = CoinageAssetState.UNTRACKED)).preClassifyCoins()

        assertEquals(emptyList<Coin>(), buckets.minted)
    }

    @Test
    fun `a coin absent from chain is minting until its mint fails`() {
        val arriving = coinOf(age = Coin.Age.Unknown, onChain = false, derivationIndex = 1)
        val failed = coinOf(age = Coin.Age.Unknown, onChain = false, derivationIndex = 2)

        val buckets = listOf(
            tracked(arriving, minterStatus = DurableTxStatus.PENDING),
            tracked(failed, minterStatus = DurableTxStatus.FAILURE),
        ).preClassifyCoins()

        assertEquals(listOf(arriving), buckets.minting)
    }

    /**
     * Presence and minter status are written by different writers — a chain subscription and the ledger — so a
     * mint can finalize while the coin still reads as absent. Reading finality as "no longer arriving" took the
     * change coin out of both buckets and the money off the screen.
     */
    @Test
    fun `a coin whose mint finalized before presence caught up is still minting`() {
        val coin = coinOf(age = Coin.Age.Unknown, onChain = false)

        val buckets = listOf(tracked(coin, minterStatus = DurableTxStatus.FINALIZED_SUCCESS))
            .preClassifyCoins()

        assertEquals(listOf(coin), buckets.minting)
    }

    /**
     * The invariant both gaps broke: only a proven-impossible mint may drop a free coin from the total. Every
     * other status has to leave it somewhere, whatever presence says.
     */
    @Test
    fun `a free coin is in the total for every minter status but failure`() {
        val counted = DurableTxStatus.entries - DurableTxStatus.FAILURE

        counted.forEach { status ->
            val onChain = coinOf(age = Coin.Age.Known(3), onChain = true, derivationIndex = 1)
            val absent = coinOf(age = Coin.Age.Unknown, onChain = false, derivationIndex = 2)

            val buckets = listOf(tracked(onChain, status), tracked(absent, status)).preClassifyCoins()

            assertEquals("minter $status", listOf(onChain, absent), buckets.total)
        }
    }

    /** A coin that will never arrive is not the user's money, so it is left out of the total entirely. */
    @Test
    fun `a coin whose mint failed is in no bucket`() {
        val failed = coinOf(age = Coin.Age.Unknown, onChain = false)

        val buckets = listOf(tracked(failed, minterStatus = DurableTxStatus.FAILURE)).preClassifyCoins()

        assertTrue(buckets.total.isEmpty())
    }

    @Test
    fun `a claimed coin is in no bucket`() {
        val claimed = coinOf(age = Coin.Age.Known(1), onChain = true)
        val state = CoinageAssetState(handedOff = true, minterStatus = null, consumerStatus = null)

        val buckets = listOf(TrackedCoin(claimed, state)).preClassifyCoins()

        assertTrue(buckets.total.isEmpty())
    }

    @Test
    fun `min privacy makes an in-recycler voucher usable at once`() {
        val voucher = voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), recyclerMembers = 0, enteredAt = null))

        val buckets = listOf(trackedVoucher(voucher)).preClassifyVouchers(minPrivacy, context())

        assertEquals(listOf(voucher), buckets.usable)
    }

    @Test
    fun `max privacy holds an in-recycler voucher back below ninety percent without a timestamp`() {
        val partial = voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), recyclerMembers = 690, enteredAt = null))

        val buckets = listOf(trackedVoucher(partial)).preClassifyVouchers(maxPrivacy, context())

        assertEquals(listOf(partial), buckets.gainingPrivacy)
        assertTrue(buckets.usable.isEmpty())
    }

    @Test
    fun `max privacy releases at ninety percent of the included ring`() {
        val vouchers = listOf(690, 691).map { members ->
            voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), members, enteredAt = null))
        }

        assertEquals(listOf(false, true), vouchers.map { maxPrivacy.isVoucherUsable(it, context()) })
    }

    @Test
    fun `balanced releases at twenty percent of the included ring`() {
        val balanced = ParametricRecyclingStrategy(RecyclingStrategyType.BALANCED.params, forcedAgeOf(FORCED_AGE))
        val vouchers = listOf(153, 154).map { members ->
            voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), members, enteredAt = null))
        }

        assertEquals(listOf(false, true), vouchers.map { balanced.isVoucherUsable(it, context()) })
    }

    @Test
    fun `an onboarding voucher is minting, whatever the ledger says about its minter`() {
        val onboarding = voucherOf(Location.Onboarding)

        val buckets = listOf(trackedVoucher(onboarding)).preClassifyVouchers(minPrivacy, context())

        assertEquals(listOf(onboarding), buckets.minting)
    }

    /** The voucher counterpart of the finalized-mint gap, and it drops money the same way. */
    @Test
    fun `a voucher whose mint finalized before its location synced is still minting`() {
        val voucher = voucherOf(Location.Unknown)

        val buckets = listOf(trackedVoucher(voucher, DurableTxStatus.FINALIZED_SUCCESS))
            .preClassifyVouchers(minPrivacy, context())

        assertEquals(listOf(voucher), buckets.minting)
    }

    @Test
    fun `a voucher whose mint failed is in no bucket`() {
        val voucher = voucherOf(Location.Unknown)

        val buckets = listOf(trackedVoucher(voucher, DurableTxStatus.FAILURE))
            .preClassifyVouchers(minPrivacy, context())

        assertTrue(buckets.total.isEmpty())
    }

    @Test
    fun `buckets never overlap`() {
        val usable = voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), FULL_RING, enteredAt = null), ringVrfKeyIndex = 1)
        val onboarding = voucherOf(Location.Onboarding, ringVrfKeyIndex = 2)

        val buckets = listOf(trackedVoucher(usable), trackedVoucher(onboarding))
            .preClassifyVouchers(maxPrivacy, context())

        assertEquals(buckets.total.size, buckets.total.distinct().size)
        assertEquals(2, buckets.total.size)
    }

    @Test
    fun `timed readiness requires thirty two members and ten full minutes in both modes`() {
        data class Case(val members: Int, val elapsed: Duration, val ready: Boolean)
        val cases = listOf(
            Case(31, 10.minutes, false),
            Case(32, 10.minutes - 1.milliseconds, false),
            Case(32, 10.minutes, true),
        )
        val enteredAt = Instant.fromEpochMilliseconds(0)
        for (type in listOf(RecyclingStrategyType.BALANCED, RecyclingStrategyType.MAX_PRIVACY)) {
            val strategy = ParametricRecyclingStrategy(type.params, forcedAgeOf(FORCED_AGE))
            for (case in cases) {
                val voucher = voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), case.members, enteredAt))
                assertEquals("$type: $case", case.ready, strategy.isVoucherUsable(voucher, context(enteredAt + case.elapsed)))
            }
        }
    }

    @Test
    fun `saturation releases small rings before either timed requirement`() {
        val voucher = voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), 9, enteredAt = null))

        assertEquals(true, maxPrivacy.isVoucherUsable(voucher, context(capacity = 10)))
    }

    @Test
    fun `maturity does not release transaction restricted vouchers`() {
        val voucher = voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), 32, Instant.fromEpochMilliseconds(0)))
        val held = TrackedVoucher(voucher, CoinageAssetState(handedOff = true, minterStatus = null, consumerStatus = null))
        val buckets = listOf(held).preClassifyVouchers(maxPrivacy, context(Instant.fromEpochMilliseconds(600000)))

        assertEquals(emptyList<RecyclerVoucher>(), buckets.total)
    }

    private fun context(now: Instant = Instant.fromEpochMilliseconds(0), capacity: Int = FULL_RING) =
        FetchedVoucherUsabilityContext(ringCapacities = mapOf(ValueExponent(1) to capacity), now = now)

    private fun tracked(
        coin: Coin,
        minterStatus: DurableTxStatus? = null,
        state: CoinageAssetState = CoinageAssetState(false, minterStatus, null),
    ) = TrackedCoin(coin, state)

    private fun trackedVoucher(
        voucher: RecyclerVoucher,
        minterStatus: DurableTxStatus = DurableTxStatus.PENDING,
    ) = TrackedVoucher(
        voucher,
        CoinageAssetState(handedOff = false, minterStatus = minterStatus, consumerStatus = null),
    )

    private fun coinOf(age: Coin.Age, onChain: Boolean, derivationIndex: Int = 0) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = age,
        isOnChain = onChain,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private fun voucherOf(location: Location, ringVrfKeyIndex: Int = 0) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(ringVrfKeyIndex),
        ringVrfPublicKey = mock(),
        recyclerValue = ValueExponent(1),
        location = location,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private companion object {
        const val FORCED_AGE = 14
    }
}
