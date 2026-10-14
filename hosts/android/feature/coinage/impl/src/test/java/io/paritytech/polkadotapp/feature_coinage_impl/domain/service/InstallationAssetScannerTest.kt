package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CheckpointBlock
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReader
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReaderFactory
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What a recovery scan is allowed to believe about the chain.
 *
 * A coin it saves becomes spendable balance with no local record of its mint, so the read behind it has to be
 * one that cannot be taken back. The best head is not: a coin read there can be reorged away, leaving a row
 * for a coin that never existed and a payment made of it that no later reading can ever settle.
 */
class InstallationAssetScannerTest {
    private val chainAssetProvider: ChainAssetProvider = mockk<ChainAssetProvider>().also {
        every { it.chainId() } returns CHAIN_ID
    }

    private val chainView: PinnedChainView = mockk<PinnedChainView>().also {
        every { it.finalizedHead } returns CheckpointBlock(blockNumber = 100, blockHash = FINALIZED_HASH)
        every { it.bestHead } returns CheckpointBlock(blockNumber = 110, blockHash = "0xbest")
    }

    private val chainViewFactory: PinnedChainViewFactory = mockk<PinnedChainViewFactory>().also {
        coEvery { it.pin(CHAIN_ID) } returns Result.success(chainView)
    }

    private val stateReader: CoinageStateReader = mockk()

    private val stateReaderFactory: CoinageStateReaderFactory = mockk<CoinageStateReaderFactory>().also {
        coEvery { it.create(any()) } returns stateReader
    }

    private val keypairDerivation: CoinKeypairDerivation = mockk<CoinKeypairDerivation>().also {
        coEvery { it.deriveKeypairs(any()) } answers { firstArg<List<Any>>().indices.map { index -> keypairOf(index) } }
    }

    private val scanner = RealInstallationAssetScanner(
        chainAssetProvider,
        chainViewFactory,
        stateReaderFactory,
        keypairDerivation,
        mockk<VoucherRingDerivation>(),
        mockk<CoinageSigningContextProvider>(),
    )

    @Test
    fun `coins are recovered from the finalized head`() = runTest {
        val readAt = slot<BlockHash>()
        coEvery { stateReader.coinsAt(capture(readAt), any()) } returns Result.success(emptyMap())

        scanner.scanCoins(TEST_INSTALLATION, startIndex = 0, count = 2)

        assertEquals(FINALIZED_HASH, readAt.captured)
    }

    @Test
    fun `a coin the finalized head holds is recovered at its derivation index`() = runTest {
        coEvery { stateReader.coinsAt(any(), any()) } returns Result.success(
            mapOf(accountOf(1) to OnChainCoinInfo(instanceId = 0, value = 3, age = 7))
        )

        val coins = scanner.scanCoins(TEST_INSTALLATION, startIndex = 0, count = 2).getOrThrow()

        assertEquals(listOf(1), coins.map { it.derivationIndex.item })
        assertEquals(listOf(3), coins.map { it.valueExponent.value })
    }

    /** No view, no read that can be trusted: the scan fails and the installation is left for the next launch. */
    @Test
    fun `a scan that cannot pin a view recovers nothing`() = runTest {
        coEvery { chainViewFactory.pin(CHAIN_ID) } returns Result.failure(IllegalStateException("no view"))

        assertTrue(scanner.scanCoins(TEST_INSTALLATION, startIndex = 0, count = 2).isFailure)
    }

    private fun accountOf(index: Int): AccountId = keypairOf(index).publicKey.intoAccountId()

    private fun keypairOf(index: Int): Keypair = mockk<Keypair>().also {
        every { it.publicKey } returns ByteArray(32) { index.toByte() }
    }

    private companion object {
        const val CHAIN_ID = "test-chain"
        const val FINALIZED_HASH = "0xfinal"
    }
}
