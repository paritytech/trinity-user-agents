package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CheckpointBlock
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.isTerminal
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReader
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReaderFactory
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUBMISSION
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * What a coin handed to a peer looks like from the sending side.
 *
 * The awkwardness this has to survive is that two independent facts decide it — whether the chain holds the
 * coin, and what the ledger says minted it — and they are observed separately. Paired at the best head they
 * can disagree for a moment, so nothing read there is allowed to be final. Only the finalized chain, where
 * both can be read at one vantage point, proves anything.
 */
class RealCoinagePaymentStatusUseCaseTest {
    private val coinageAssetsUseCase: CoinageAssetsUseCase = mockk()
    private val chainViewFactory: PinnedChainViewFactory = mockk()
    private val chainView: PinnedChainView = mockk()
    private val stateReaderFactory: CoinageStateReaderFactory = mockk()
    private val stateReader: CoinageStateReader = mockk()

    private val chainAssetProvider: ChainAssetProvider = mockk<ChainAssetProvider>().also {
        every { it.chainId() } returns "test-chain"
    }

    private val useCase = RealCoinagePaymentStatusUseCase(
        coinageAssetsUseCase,
        chainViewFactory,
        stateReaderFactory,
        chainAssetProvider,
    )

    @Test
    fun `a coin still on chain is waiting for the peer to take it`() = runTest {
        givenCoin(onChain = true, everSeen = true, minter = PENDING_SUCCESS, atFinalized = PRESENT)

        assertEquals(CoinagePaymentStatus.AwaitingClaim, statusOfCoin())
    }

    /**
     * The mint finalized and the finalized chain no longer holds the coin. Nothing but the peer's claim
     * removes it, and neither fact can be taken back, so this is the one reading that settles a payment.
     */
    @Test
    fun `a coin gone from the finalized chain is proven claimed`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = FINALIZED_SUCCESS, atFinalized = ABSENT)

        assertEquals(CoinagePaymentStatus.Claimed(finalized = true), statusOfCoin())
    }

    /**
     * The local row says the coin is gone and the mint has finalized, but the finalized chain still holds it
     * — the row had simply not caught up.
     *
     * Believing the row here is what let a 35.1 payment report "claimed 2.xx" while its coins sat untouched.
     * The two facts are read independently, so the pairing can be a moment out of date; the finalized chain
     * is the arbiter.
     */
    @Test
    fun `a coin the finalized chain still holds is not claimed however stale the local row`() = runTest {
        givenCoin(onChain = false, everSeen = false, minter = FINALIZED_SUCCESS, atFinalized = PRESENT)

        assertEquals(CoinagePaymentStatus.AwaitingClaim, statusOfCoin())
    }

    /**
     * The app was closed moments after sending and the peer claimed while it was gone, so nothing local ever
     * saw the coin on chain. The finalized chain still settles it: the mint executed, the coin is not there.
     *
     * Without asking, this coin would be stuck reading as "still sending" for good, and its payment would be
     * reprocessed on every launch.
     */
    @Test
    fun `a coin claimed while the app was closed is still proven claimed`() = runTest {
        givenCoin(onChain = false, everSeen = false, minter = FINALIZED_SUCCESS, atFinalized = ABSENT)

        assertEquals(CoinagePaymentStatus.Claimed(finalized = true), statusOfCoin())
    }

    /**
     * Gone from the best head with a mint that is only in a best-chain block. Both halves can still be taken
     * back, so it reads as claimed without being proven — enough to show, not enough to close a payment on.
     */
    @Test
    fun `a coin gone with the mint only in a block is claimed but unproven`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = PENDING_SUCCESS, atFinalized = PRESENT)

        assertEquals(CoinagePaymentStatus.Claimed(finalized = false), statusOfCoin())
    }

    /**
     * The peer has taken the coin: we saw it on chain and then saw it gone. The claim is only in a best
     * block, so the finalized chain still holds it — but that says nothing about what has happened since.
     *
     * Waiting for the claim to finalize before saying anything would skip the provisional state entirely,
     * since by then the finalized chain has lost the coin and it is proven.
     */
    @Test
    fun `a coin the peer has taken reads as claimed before the claim finalizes`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = FINALIZED_SUCCESS, atFinalized = PRESENT)

        assertEquals(CoinagePaymentStatus.Claimed(finalized = false), statusOfCoin())
    }

    /** The mint is proven never to have executed, so the key the peer holds controls nothing. */
    @Test
    fun `a coin whose mint failed never existed`() = runTest {
        givenCoin(onChain = false, everSeen = false, minter = FAILURE, atFinalized = ABSENT)

        assertEquals(CoinagePaymentStatus.Failed, statusOfCoin())
    }

    @Test
    fun `a coin whose mint has not reached a block yet is still on its way`() = runTest {
        givenCoin(onChain = false, everSeen = false, minter = PENDING, atFinalized = ABSENT)

        assertEquals(CoinagePaymentStatus.Detecting, statusOfCoin())
    }

    /**
     * The payment is saved and its transaction waits to be built — or to be built again after an attempt that
     * could never land. The coin does not exist yet, and nothing has decided it never will.
     */
    @Test
    fun `a coin whose minter is waiting to be built reads as detecting`() = runTest {
        givenCoin(onChain = false, everSeen = false, minter = PENDING_SUBMISSION, atFinalized = ABSENT)

        val status = statusOfCoin()

        assertEquals(CoinagePaymentStatus.Detecting, status)
        assertFalse("a payment waiting on a rebuild must stay open", status.isTerminal)
    }

    /**
     * Absent, with a mint in a block, and nothing has ever seen the coin on chain — so its absence is
     * ignorance rather than evidence, and guessing "claimed" from it would be guessing from nothing.
     */
    @Test
    fun `a coin nothing has ever seen is not guessed to be claimed`() = runTest {
        givenCoin(onChain = false, everSeen = false, minter = PENDING_SUCCESS, atFinalized = ABSENT)

        assertEquals(CoinagePaymentStatus.Detecting, statusOfCoin())
    }

    /**
     * An exact-coins payment hands over coins whose mint finalized long ago, and the claim is the peer's own
     * transaction, so nothing local changes when it finalizes. Only a new finalized head can prove it.
     */
    @Test
    fun `a claim finalizing with nothing local changing is proven at the next finalized head`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = FINALIZED_SUCCESS, atFinalized = PRESENT)
        every { chainViewFactory.finalizedHeads(any()) } returns flowOf(BlockNumber(101.toBigInteger()))
        coEvery { stateReader.coinsAt(any(), any()) } returnsMany listOf(
            Result.success(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 0))),
            Result.success(emptyMap()),
        )

        val statuses = useCase.subscribeStatuses(listOf(ACCOUNT)).take(2).toList().map { it.getValue(ACCOUNT).status }

        assertEquals(listOf(CoinagePaymentStatus.Claimed(finalized = false), CoinagePaymentStatus.Claimed(finalized = true)), statuses)
    }

    /**
     * A coin recovered from a previous installation's backup, once the peer has taken it.
     *
     * No entry of ours minted it, so the ledger is what decides its minter status: it hands back
     * FINALIZED_SUCCESS, since recovery only ever saves what the finalized chain already held. That is all
     * this use case needs to settle the payment — read as an unfinished mint it would sit at Detecting for
     * good, and its payment would never close.
     */
    @Test
    fun `a coin recovered from a previous installation is proven claimed once it is gone`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = FINALIZED_SUCCESS, atFinalized = ABSENT, installation = PREVIOUS_INSTALLATION)

        assertEquals(CoinagePaymentStatus.Claimed(finalized = true), statusOfCoin())
    }

    /**
     * Nothing is guessed from a minter the ledger reports as unknown: which mints count as settled is the
     * ledger's rule, and a status invented here would apply it a second time and differently.
     */
    @Test
    fun `a coin the ledger reports no minter for is still detecting`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = null, atFinalized = ABSENT)

        assertEquals(CoinagePaymentStatus.Detecting, statusOfCoin())
    }

    /** A finalized read that cannot be taken proves nothing, and must not be read as the coin being gone. */
    @Test
    fun `a coin whose finalized read fails is not proven claimed`() = runTest {
        givenCoin(onChain = false, everSeen = true, minter = FINALIZED_SUCCESS, atFinalized = UNREADABLE)

        assertEquals(CoinagePaymentStatus.Claimed(finalized = false), statusOfCoin())
    }

    private suspend fun statusOfCoin(): CoinagePaymentStatus =
        useCase.subscribeStatuses(listOf(ACCOUNT)).first().getValue(ACCOUNT).status

    private fun givenCoin(
        onChain: Boolean,
        everSeen: Boolean,
        minter: DurableTxStatus?,
        atFinalized: FinalizedRead,
        installation: CoinageInstallationId = TEST_INSTALLATION,
    ) {
        val coin = Coin(
            derivationIndex = CoinageKeyIndex(installation, 0),
            valueExponent = ValueExponent(3),
            // An age is kept once the chain has been seen to hold the coin, and never cleared after.
            age = if (everSeen) Coin.Age.Known(0) else Coin.Age.Unknown,
            isOnChain = onChain,
            accountId = ACCOUNT,
            provenance = CoinProvenance.UNKNOWN,
        )
        val tracked = TrackedCoin(
            coin = coin,
            // Handed off is what makes this a payment: the key has left the device.
            state = CoinageAssetState(handedOff = true, minterStatus = minter, consumerStatus = null),
        )

        every { coinageAssetsUseCase.subscribeCoinsBy(any()) } returns flowOf(listOf(tracked))
        every { chainViewFactory.finalizedHeads(any()) } returns emptyFlow()

        coEvery { chainViewFactory.pin(any()) } returns when (atFinalized) {
            UNREADABLE -> Result.failure(IllegalStateException("no view"))
            else -> Result.success(chainView)
        }
        every { chainView.finalizedHead } returns CheckpointBlock(blockNumber = 100, blockHash = "0xfinal")
        coEvery { stateReaderFactory.create(any()) } returns stateReader
        coEvery { stateReader.coinsAt(any(), any()) } returns Result.success(
            mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 0).takeIf { atFinalized == PRESENT })
        )
    }

    private enum class FinalizedRead { PRESENT, ABSENT, UNREADABLE }

    private companion object {
        val PRESENT = FinalizedRead.PRESENT
        val ABSENT = FinalizedRead.ABSENT
        val UNREADABLE = FinalizedRead.UNREADABLE

        val ACCOUNT: AccountId = byteArrayOf(7).toDataByteArray()

        val PREVIOUS_INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x01 }.toDataByteArray())
    }
}
