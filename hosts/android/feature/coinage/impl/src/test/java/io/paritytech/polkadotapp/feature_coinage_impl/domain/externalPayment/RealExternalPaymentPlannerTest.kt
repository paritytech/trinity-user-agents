package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.PowerOfTwoConversion
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinageAssetSelector
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.SpendScope
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which funds pay an external payment: private vouchers, then any voucher, then coins recycled for the rest.
 * Anything beyond private vouchers is only reached for once the user has confirmed giving up the privacy.
 *
 * A voucher or coin of exponent `e` is worth 2^e planks.
 */
class RealExternalPaymentPlannerTest {
    private val assetSelector: CoinageAssetSelector = mockk()
    private val converter: CoinageBalanceConverterUseCase = mockk {
        coEvery { create() } returns Result.success(PowerOfTwoConversion)
    }

    private val planner = RealExternalPaymentPlanner(assetSelector, converter)

    @Test
    fun `private vouchers that cover the amount are offboarded`() = runBlocking<Unit> {
        val big = voucher(1, exponent = 3)
        val small = voucher(2, exponent = 1)
        givenFunds(privateVouchers = listOf(small, big))

        val plan = planner.plan(planks(6)).getOrThrow()

        assertTrue(plan is ExternalPaymentPlan.Ready)
        plan as ExternalPaymentPlan.Ready
        assertEquals(listOf(big), plan.offboarding.vouchers)
        assertEquals(planks(2), plan.offboarding.surplus)
    }

    /**
     * A coin loaded only to be unloaded right away is no more private than a voucher still gaining privacy, so
     * the vouchers pay and no coin waits on a recycling round.
     */
    @Test
    fun `vouchers still gaining privacy pay before any coin is loaded`() = runBlocking<Unit> {
        val privateVoucher = voucher(1, exponent = 2)
        val gainingPrivacy = voucher(2, exponent = 4)
        givenFunds(
            privateVouchers = listOf(privateVoucher),
            onChainVouchers = listOf(privateVoucher, gainingPrivacy),
            recyclableCoins = listOf(coin(3, exponent = 3)),
        )

        val plan = planner.plan(planks(10)).getOrThrow()

        assertTrue(plan is ExternalPaymentPlan.Ready)
        coVerify(exactly = 0) { assetSelector.getRecyclableCoins() }
    }

    /** Largest-first alone would take both 8s; starting from the private 4 gives up the privacy of only one. */
    @Test
    fun `private vouchers are spent before those still gaining privacy`() = runBlocking<Unit> {
        val privateVoucher = voucher(1, exponent = 2)
        val gainingPrivacy = voucher(2, exponent = 3)
        givenFunds(
            privateVouchers = listOf(privateVoucher),
            onChainVouchers = listOf(voucher(3, exponent = 3), privateVoucher, gainingPrivacy),
        )

        val plan = planner.plan(planks(12)).getOrThrow()

        assertTrue(plan is ExternalPaymentPlan.Ready)
        plan as ExternalPaymentPlan.Ready
        assertEquals(privateVoucher, plan.offboarding.vouchers.first())
        assertEquals(2, plan.offboarding.vouchers.size)
        assertEquals(planks(0), plan.offboarding.surplus)
    }

    /** Every voucher is offboarded as it is, and coins are recycled only for what they lack together. */
    @Test
    fun `recyclable coins make up what all vouchers lack`() = runBlocking<Unit> {
        val privateVoucher = voucher(1, exponent = 1)
        val gainingPrivacy = voucher(2, exponent = 2)
        val coinToLoad = coin(3, exponent = 3)
        givenFunds(
            privateVouchers = listOf(privateVoucher),
            onChainVouchers = listOf(privateVoucher, gainingPrivacy),
            recyclableCoins = listOf(coin(4, exponent = 1), coinToLoad),
        )

        val plan = planner.plan(planks(12)).getOrThrow()

        assertTrue(plan is ExternalPaymentPlan.LoadCoins)
        plan as ExternalPaymentPlan.LoadCoins
        assertEquals(listOf(coinToLoad), plan.coinsToLoad)
        assertEquals(listOf(privateVoucher, gainingPrivacy), plan.exactVouchers)
    }

    @Test
    fun `only private vouchers pay privately`() = runBlocking<Unit> {
        givenFunds(privateVouchers = listOf(voucher(1, exponent = 3)))

        assertTrue(planner.canPayPrivately(planks(8)).getOrThrow())
        assertTrue(!planner.canPayPrivately(planks(9)).getOrThrow())
    }

    @Test
    fun `an amount nothing covers is reported with what there was`() = runBlocking<Unit> {
        givenFunds(onChainVouchers = listOf(voucher(1, exponent = 1)), recyclableCoins = listOf(coin(2, exponent = 2)))

        val plan = planner.plan(planks(16)).getOrThrow()

        assertEquals(
            ExternalPaymentPlan.NotEnoughAmount(
                activeVouchers = planks(2),
                activeCoins = planks(4),
                deficitToCoverWithCoins = planks(14),
            ),
            plan,
        )
    }

    private fun givenFunds(
        privateVouchers: List<RecyclerVoucher> = emptyList(),
        onChainVouchers: List<RecyclerVoucher> = privateVouchers,
        recyclableCoins: List<Coin> = emptyList(),
    ) {
        coEvery { assetSelector.getSelectableVouchers(SpendScope.SPENDABLE) } returns privateVouchers
        coEvery { assetSelector.getOnChainSpendableVouchers() } returns onChainVouchers
        coEvery { assetSelector.getRecyclableCoins() } returns recyclableCoins
    }

    private fun voucher(index: Int, exponent: Int) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(index),
        ringVrfPublicKey = byteArrayOf(index.toByte()).toDataByteArray(),
        recyclerValue = ValueExponent(exponent),
        location = RecyclerVoucher.Location.Unknown,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private fun coin(index: Int, exponent: Int) = Coin(
        derivationIndex = testKey(index),
        valueExponent = ValueExponent(exponent),
        age = Coin.Age.Known(0),
        isOnChain = true,
        accountId = byteArrayOf(index.toByte()).intoAccountId(),
        provenance = CoinProvenance.UNKNOWN,
    )
}
