package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreRepository
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.HeadKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.LedgerView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CoinageInstallationRegistrationOracleTest {
    private val repository = mockk<AccountDataStoreRepository>()

    private val oracle = CoinageInstallationRegistrationOracle(
        knownChains = KnownChains(people = "people", assetHub = "asset-hub", bulletIn = "bullet-in", hydration = null),
        dataStoreRepository = repository,
    )

    @Test
    fun `the domain lives on asset hub`() {
        assertEquals("asset-hub", oracle.chainId)
    }

    @Test
    fun `a registration whose installation is listed took effect`() = runBlocking<Unit> {
        listed(CONTRACT, FINALIZED_HASH, setOf(TEST_INSTALLATION))
        listed(CONTRACT, BEST_HASH, setOf(TEST_INSTALLATION))

        val tx = registrationOf(TEST_INSTALLATION)
        val scope = openPass(tx)

        assertTrue(scope.provenCompleted(tx, HeadKind.FINALIZED))
    }

    @Test
    fun `a registration listed only at the best head is not yet final`() = runBlocking<Unit> {
        listed(CONTRACT, FINALIZED_HASH, emptySet())
        listed(CONTRACT, BEST_HASH, setOf(TEST_INSTALLATION))

        val tx = registrationOf(TEST_INSTALLATION)
        val scope = openPass(tx)

        assertTrue(scope.provenCompleted(tx, HeadKind.BEST))
        assertFalse(scope.provenCompleted(tx, HeadKind.FINALIZED))
        assertTrue(scope.provenNotCompleted(tx, HeadKind.FINALIZED))
    }

    @Test
    fun `another installation being listed says nothing about this one`() = runBlocking<Unit> {
        listed(CONTRACT, FINALIZED_HASH, setOf(OTHER_INSTALLATION))
        listed(CONTRACT, BEST_HASH, setOf(OTHER_INSTALLATION))

        val tx = registrationOf(TEST_INSTALLATION)
        val scope = openPass(tx)

        assertFalse(scope.provenCompleted(tx, HeadKind.BEST))
        assertTrue(scope.provenNotCompleted(tx, HeadKind.BEST))
    }

    @Test
    fun `each registration is judged on the contract its group names`() = runBlocking<Unit> {
        listed(CONTRACT, FINALIZED_HASH, setOf(TEST_INSTALLATION))
        listed(CONTRACT, BEST_HASH, setOf(TEST_INSTALLATION))
        listed(OTHER_CONTRACT, FINALIZED_HASH, emptySet())
        listed(OTHER_CONTRACT, BEST_HASH, emptySet())

        val onFirst = registrationOf(TEST_INSTALLATION, contract = CONTRACT, id = 1)
        val onSecond = registrationOf(TEST_INSTALLATION, contract = OTHER_CONTRACT, id = 2)
        val scope = openPass(onFirst, onSecond)

        assertTrue(scope.provenCompleted(onFirst, HeadKind.FINALIZED))
        assertTrue(scope.provenNotCompleted(onSecond, HeadKind.FINALIZED))
    }

    @Test
    fun `a failed read decides nothing`() = runBlocking<Unit> {
        coEvery { repository.fetchRegisteredInstallations(any(), any()) } returns Result.failure(IllegalStateException("unreachable node"))

        val tx = registrationOf(TEST_INSTALLATION)
        val scope = openPass(tx)

        HeadKind.entries.forEach { head ->
            assertFalse(scope.provenCompleted(tx, head))
            assertFalse(scope.provenNotCompleted(tx, head))
        }
    }

    @Test
    fun `a transaction whose group names no target decides nothing`() = runBlocking<Unit> {
        listed(CONTRACT, FINALIZED_HASH, setOf(TEST_INSTALLATION))
        listed(CONTRACT, BEST_HASH, setOf(TEST_INSTALLATION))

        listOf(
            OperationGroupId("not-a-target"),
            OperationGroupId(TEST_INSTALLATION.value.value.toHexString(withPrefix = true)),
        ).forEach { groupId ->
            val tx = entry(id = 1, groupId = groupId)
            val scope = openPass(tx)

            assertFalse(scope.provenCompleted(tx, HeadKind.FINALIZED))
            assertFalse(scope.provenNotCompleted(tx, HeadKind.FINALIZED))
        }
    }

    @Test
    fun `one read per head per contract however many registrations are live`() = runBlocking<Unit> {
        listed(CONTRACT, FINALIZED_HASH, setOf(TEST_INSTALLATION))
        listed(CONTRACT, BEST_HASH, setOf(TEST_INSTALLATION))

        val transactions = List(25) { registrationOf(TEST_INSTALLATION, id = it.toLong()) }
        oracle.openPass(transactions, ledgerOf(transactions), view()).getOrThrow()

        coVerify(exactly = 2) { repository.fetchRegisteredInstallations(any(), any()) }
    }

    private suspend fun openPass(vararg transactions: DurableTxEntry): TxCompletionOracle.PassScope {
        val list = transactions.toList()
        return oracle.openPass(list, ledgerOf(list), view()).getOrThrow()
    }

    private fun registrationOf(installation: CoinageInstallationId, contract: EvmAccountId = CONTRACT, id: Long = 1) =
        entry(id = id, groupId = InstallationRegistrationTarget(contract, installation).registrationGroup())

    private fun entry(id: Long, groupId: OperationGroupId?) = DurableTxEntry(
        id = DurableTxId(id),
        domainId = COINAGE_INSTALLATION_DOMAIN,
        groupId = groupId,
        txHash = "0xtx$id",
        checkpoint = CheckpointBlock(100, "0xcheckpoint"),
        mortalityBlocks = 64,
        status = DurableTxStatus.PENDING,
        successDetectedAt = null,
    )

    private fun ledgerOf(entries: List<DurableTxEntry>) = object : LedgerView {
        override val transactions = entries

        override fun statusOf(id: DurableTxId) = entries.firstOrNull { it.id == id }?.status
    }

    private fun view() = mockk<PinnedChainView> {
        every { finalizedHead } returns CheckpointBlock(130, FINALIZED_HASH)
        every { bestHead } returns CheckpointBlock(140, BEST_HASH)
    }

    private fun listed(contract: EvmAccountId, at: BlockHash, installations: Set<CoinageInstallationId>) {
        coEvery { repository.fetchRegisteredInstallations(contract, at) } returns Result.success(installations)
    }

    private companion object {
        const val FINALIZED_HASH = "0xfinalized"
        const val BEST_HASH = "0xbest"

        val CONTRACT = ByteArray(20) { 0x0c }.toDataByteArray()
        val OTHER_CONTRACT = ByteArray(20) { 0x0d }.toDataByteArray()
        val OTHER_INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x11 }.toDataByteArray())
    }
}
