package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWire
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.toDomain
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import java.math.BigDecimal
import kotlin.time.Duration.Companion.hours
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Turning what coinage detected into what a product is told.
 *
 * The two vocabularies do not line up: coinage reports what actually landed, which can fall short of what
 * the product asked for, and it has a "part landed, rest retrying" state products are given no name for.
 * Getting the translation wrong is how a top-up still in progress gets reported as terminally short.
 */
@OptIn(ExperimentalTime::class)
class RealExecuteTopUpUseCaseTest {
    private val claimReceivedCoinsUseCase = FakeClaimReceivedCoinsUseCase()
    private val onboardingUseCase = FakeOnboardingUseCase()
    private val chainAssetProvider: ChainAssetProvider = mock()

    private val transactionService: CoinageTransactionService = mock()
    private val assetValueUseCase: CoinageAssetValueUseCase = mock()

    private val useCase = RealExecuteTopUpUseCase(
        claimReceivedCoinsUseCase = claimReceivedCoinsUseCase,
        onboardingUseCase = onboardingUseCase,
        transactionService = transactionService,
        assetValueUseCase = assetValueUseCase,
        chainAssetProvider = chainAssetProvider,
    )

    @Before
    fun withResolvedAsset() = runTest {
        val asset = mock<Chain.Asset>()
        whenever(asset.precision).thenReturn(10)
        whenever(chainAssetProvider.asset()).thenReturn(asset)
    }

    // ---- what the product is told ----

    @Test
    fun `Claimed once everything the product asked for is finalized`() = runTest {
        onboardingUseCase.emits(
            CoinageTransferDetection.Claiming,
            CoinageTransferDetection.Claimed(REQUESTED, finalized = true),
        )

        assertEquals(
            listOf(TopUpStatus.Claiming, TopUpStatus.Claimed(finalized = true)),
            statusesOf(onboardOperation()),
        )
    }

    /** An inclusion is worth reporting; it is simply not the last word until it finalizes. */
    @Test
    fun `Claimed reports the inclusion before the finality`() = runTest {
        onboardingUseCase.emits(
            CoinageTransferDetection.Claimed(REQUESTED, finalized = false),
            CoinageTransferDetection.Claimed(REQUESTED, finalized = true),
        )

        assertEquals(
            listOf(TopUpStatus.Claimed(finalized = false), TopUpStatus.Claimed(finalized = true)),
            statusesOf(onboardOperation()),
        )
    }

    /**
     * Coinage has a state for "part of it landed and the rest is being retried". Products do not, and it is
     * a claim in progress either way — reporting the running total as a partial claim would call a top-up
     * terminally short while the rest was still on its way.
     */
    @Test
    fun `a claim held up part way through still reports as claiming`() = runTest {
        claimReceivedCoinsUseCase.emits(CoinageTransferDetection.ClaimingRest(80.intoBalance()))

        assertEquals(listOf(TopUpStatus.Claiming), statusesOf(coinsOperation()))
    }

    /**
     * Everything that was going to be claimed has been, and it is short of what the product asked for — an
     * underfunded account, or coins worth less than the sender said. Only finality may call that partial.
     */
    @Test
    fun `a finalized shortfall is reported as partial`() = runTest {
        onboardingUseCase.emits(CoinageTransferDetection.Claimed(80.intoBalance(), finalized = true))

        assertEquals(listOf(TopUpStatus.ClaimedPartially(80.intoBalance())), statusesOf(onboardOperation()))
    }

    /** The same shortfall before finality: the rest can still arrive, so it is not partial yet. */
    @Test
    fun `a shortfall that has not finalized is still claiming`() = runTest {
        onboardingUseCase.emits(CoinageTransferDetection.Claimed(80.intoBalance(), finalized = false))

        assertEquals(listOf(TopUpStatus.Claiming), statusesOf(onboardOperation()))
    }

    @Test
    fun `a claim that ended short is reported as partial`() = runTest {
        claimReceivedCoinsUseCase.emits(CoinageTransferDetection.ClaimedPartially(80.intoBalance()))

        assertEquals(listOf(TopUpStatus.ClaimedPartially(80.intoBalance())), statusesOf(coinsOperation()))
    }

    @Test
    fun `a claim that never landed is reported as not claimed`() = runTest {
        claimReceivedCoinsUseCase.emits(CoinageTransferDetection.NotClaimed)

        assertEquals(listOf(TopUpStatus.NotClaimed), statusesOf(coinsOperation()))
    }

    /**
     * Coinage re-evaluates on every ledger update, so it repeats itself freely. A product subscribing to the
     * status should not be woken for a status it already has.
     */
    @Test
    fun `a status the product already has is not reported again`() = runTest {
        claimReceivedCoinsUseCase.emits(
            CoinageTransferDetection.Claiming,
            CoinageTransferDetection.ClaimingRest(80.intoBalance()),
            CoinageTransferDetection.Claiming,
        )

        assertEquals(listOf(TopUpStatus.Claiming), statusesOf(coinsOperation()))
    }

    /**
     * An amount below the smallest denomination cannot be claimed whole, so the host claims
     * `amount - amount % 2^min_exponent` and the shortfall is the rounding.
     *
     * The product asked for more than it got, which is exactly what ClaimedPartially says — reporting a
     * rounded-down claim as Claimed would tell a product it had been credited the amount it named.
     */
    @Test
    fun `an amount rounded down to the smallest denomination reports as partial`() = runTest {
        onboardingUseCase.emits(CoinageTransferDetection.Claimed(96.intoBalance(), finalized = true))

        assertEquals(
            listOf(TopUpStatus.ClaimedPartially(96.intoBalance())),
            statusesOf(onboardOperation()),
        )
    }

    // ---- how the operation is identified and bounded ----

    /** The product's own id names the coinage group, so a resumed top-up rejoins rather than pays twice. */
    @Test
    fun `the coinage group is named after the product and its top-up id`() = runTest {
        claimReceivedCoinsUseCase.emits(CoinageTransferDetection.NotClaimed)

        statusesOf(coinsOperation())

        assertEquals(
            CoinageOperationGroupId("top up:${PRODUCT.value}:${ID.asHex()}"),
            claimReceivedCoinsUseCase.groupId,
        )
    }

    /**
     * A top-up id is a product's own opaque bytes, so two products may each register the same one. Their
     * transactions must not share a group: each product's claims would then count towards the other's total.
     */
    @Test
    fun `the same top-up id under a different product names a different group`() = runTest {
        claimReceivedCoinsUseCase.emits(CoinageTransferDetection.NotClaimed)

        statusesOf(coinsOperation())
        val mine = claimReceivedCoinsUseCase.groupId

        statusesOf(coinsOperation(productId = OTHER_PRODUCT))

        assertNotEquals(mine, claimReceivedCoinsUseCase.groupId)
    }

    /**
     * The window runs from when the operation opened, not from this attempt. A resumed top-up that was
     * handed a fresh hour every time it was picked up could never terminally fail.
     */
    @Test
    fun `the retry window runs from when the operation opened`() = runTest {
        onboardingUseCase.emits(CoinageTransferDetection.NotClaimed)

        statusesOf(onboardOperation(startedAt = OPENED))

        assertEquals(OPENED + 1.hours, onboardingUseCase.retryUntil)
    }

    // ---- harness ----

    private suspend fun statusesOf(fixture: OperationWithSource): List<TopUpStatus> =
        useCase.execute(fixture.operation, resolved(fixture.source)).toList()

    private fun onboardOperation(startedAt: Instant = OPENED) = operation(
        source = PaymentTopUpSource.ProductAccount(DerivationIndexWire.Plain(0u).toDomain().getOrThrow()),
        startedAt = startedAt,
    )

    private fun coinsOperation(productId: ProductId = PRODUCT) =
        operation(source = PaymentTopUpSource.Coins(emptyList()), productId = productId)

    private fun operation(
        source: PaymentTopUpSource,
        startedAt: Instant = OPENED,
        productId: ProductId = PRODUCT,
    ) = OperationWithSource(
        operation = TopUpOperation(
            id = ID,
            productId = productId,
            amount = REQUESTED,
            startedAt = startedAt,
            outcome = null,
        ),
        source = source,
    )

    private class OperationWithSource(val operation: TopUpOperation, val source: PaymentTopUpSource)

    private fun resolved(source: PaymentTopUpSource): TopUpSource = when (source) {
        is PaymentTopUpSource.Coins -> TopUpSource.Coins(emptyList())
        else -> TopUpSource.Onboard(TransactionSignerSource.FromAccount(mock<MetaAccount>()))
    }

    private companion object {
        val DOT_TLD: DotNsTld = requireNotNull(DotNsTld.parse("dot"))

        val ID = topUpId("topup-1")
        val PRODUCT: ProductId = ProductId.fromString("alice.dot", DOT_TLD).getOrThrow()
        val OTHER_PRODUCT: ProductId = ProductId.fromString("bob.dot", DOT_TLD).getOrThrow()
        val REQUESTED: Balance = 100.intoBalance()
        val OPENED: Instant = Instant.fromEpochSeconds(1_000_000)
    }
}

/** A fake rather than a mock: both use cases take value classes, which Mockito unwraps. */
@OptIn(ExperimentalTime::class)
private class FakeClaimReceivedCoinsUseCase : ClaimReceivedCoinsUseCase {
    private var states: List<CoinageTransferDetection> = emptyList()

    var groupId: CoinageOperationGroupId? = null
        private set

    fun emits(vararg states: CoinageTransferDetection) {
        this.states = states.toList()
    }

    override fun claim(
        coinKeys: List<CoinPrivateKey>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> {
        this.groupId = groupId

        return flow { states.forEach { emit(it) } }
    }
}

@OptIn(ExperimentalTime::class)
private class FakeOnboardingUseCase : OnboardingUseCase {
    private var states: List<CoinageTransferDetection> = emptyList()

    var retryUntil: Instant? = null
        private set

    fun emits(vararg states: CoinageTransferDetection) {
        this.states = states.toList()
    }

    override suspend fun onboard(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
    ): Result<Unit> = Result.success(Unit)

    override fun onboardDurably(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> {
        this.retryUntil = retryUntil

        return flow { states.forEach { emit(it) } }
    }
}
