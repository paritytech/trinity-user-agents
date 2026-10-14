package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.PreviousInstallation
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.DeepRecoveryCompletedStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CoinageBackupServiceTest {
    private val installations = InMemoryInstallations(current = TEST_INSTALLATION)
    private val deepRecoveryCompleted = InMemoryFlag()

    private var registered: Result<Set<CoinageInstallationId>> = Result.success(emptySet())
    private var coinsOnChain = emptySet<CoinageKeyIndex>()
    private var scanFailing = false
    private val scannedInstallations = mutableSetOf<CoinageInstallationId>()
    private val savedCoins = mutableListOf<Coin>()

    private val dataStore = mockk<AccountDataStoreRepository> {
        coEvery { fetchRegisteredInstallations(CONTRACT, null) } answers { registered }
    }

    // Stubbed per installation: a value-class `any()` matcher cannot build a CoinageInstallationId past its size check.
    private val scanner = mockk<InstallationAssetScanner> {
        listOf(TEST_INSTALLATION, PREVIOUS, ANOTHER_PREVIOUS).forEach { installation ->
            coEvery { scanCoins(installation, any(), any()) } answers {
                val startIndex = secondArg<Int>()
                scannedInstallations += installation

                if (scanFailing) {
                    Result.failure(IllegalStateException("node went away"))
                } else {
                    val batch = (startIndex until startIndex + thirdArg<Int>()).map { CoinageKeyIndex(installation, it) }
                    Result.success(batch.filter { it in coinsOnChain }.map(::coinAt))
                }
            }
            coEvery { scanVouchers(installation, any(), any()) } answers {
                scannedInstallations += installation
                if (scanFailing) Result.failure(IllegalStateException("node went away")) else Result.success(emptyList())
            }
        }
    }
    private val coinRepository = mockk<CoinRepository>(relaxed = true) {
        coEvery { saveAll(any()) } answers { savedCoins += firstArg<List<Coin>>() }
    }
    private val voucherRepository = mockk<VoucherRepository>(relaxed = true)

    private val service = RealCoinageBackupService(
        installationRepository = installations,
        dataStoreConfigProvider = mockk<AccountDataStoreConfigProvider> { coEvery { contractAddress() } returns Result.success(CONTRACT) },
        dataStoreRepository = dataStore,
        assetScanner = scanner,
        coinsRepository = coinRepository,
        voucherRepository = voucherRepository,
        deepRecoveryCompletedStorage = deepRecoveryCompleted,
    )

    @Test
    fun `the current installation is never scanned`() = runTest {
        registered = Result.success(setOf(TEST_INSTALLATION, PREVIOUS))

        start()

        assertEquals(setOf(PREVIOUS), scannedInstallations)
    }

    @Test
    fun `coins found under a previous installation keep that installation and their absolute index`() = runTest {
        registered = Result.success(setOf(PREVIOUS))
        coinsOnChain = setOf(CoinageKeyIndex(PREVIOUS, 1_234))

        start()

        assertEquals(listOf(CoinageKeyIndex(PREVIOUS, 1_234)), savedCoins.map { it.derivationIndex })
    }

    @Test
    fun `the scan stops after four empty batches in a row, not four in total`() = runTest {
        registered = Result.success(setOf(PREVIOUS))
        // Non-empty batches 0 and 3 and 6; with a counter that never reset, batch 6 would be missed.
        coinsOnChain = setOf(10, 1_600, 3_100).map { CoinageKeyIndex(PREVIOUS, it) }.toSet()

        start()

        assertEquals(listOf(10, 1_600, 3_100), savedCoins.map { it.derivationIndex.item })
        assertEquals(3_100 / BATCH_SIZE * BATCH_SIZE + (EMPTY_BATCHES + 1) * BATCH_SIZE, installations.previous(PREVIOUS).coinScanNextIndex)
    }

    @Test
    fun `a scanned installation is not scanned again on the next launch`() = runTest {
        registered = Result.success(setOf(PREVIOUS))
        start()
        scannedInstallations.clear()

        start()

        assertTrue(scannedInstallations.isEmpty())
    }

    @Test
    fun `an installation registered after the last launch is scanned on this one`() = runTest {
        registered = Result.success(setOf(PREVIOUS))
        start()
        scannedInstallations.clear()

        registered = Result.success(setOf(PREVIOUS, ANOTHER_PREVIOUS))
        start()

        assertEquals(setOf(ANOTHER_PREVIOUS), scannedInstallations)
    }

    @Test
    fun `a failed contract read still scans installations already known`() = runTest {
        installations.addPrevious(listOf(PREVIOUS))
        registered = Result.failure(IllegalStateException("unreachable"))

        start()

        assertEquals(setOf(PREVIOUS), scannedInstallations)
    }

    @Test
    fun `an installation whose scan failed is retried on the next launch`() = runTest {
        registered = Result.success(setOf(PREVIOUS))
        scanFailing = true
        start()
        assertFalse(installations.previous(PREVIOUS).initialScanCompleted)

        scanFailing = false
        start()

        assertTrue(installations.previous(PREVIOUS).initialScanCompleted)
    }

    @Test
    fun `with previous installations recovered the user is asked to confirm the balance`() = runTest {
        registered = Result.success(setOf(PREVIOUS))

        start()

        assertEquals(BackupProgress.Initial.Completed, service.subscribeProgress().value())
    }

    @Test
    fun `with nothing to recover there is nothing to confirm`() = runTest {
        registered = Result.success(setOf(TEST_INSTALLATION))

        start()

        assertEquals(BackupProgress.Completed, service.subscribeProgress().value())
        coVerify(exactly = 0) { coinRepository.saveAll(any()) }
    }

    @Test
    fun `deep search continues every previous installation from where it stopped`() = runTest {
        registered = Result.success(setOf(PREVIOUS, ANOTHER_PREVIOUS))
        start()
        val resumeFrom = installations.previous(PREVIOUS).coinScanNextIndex

        with(ComputationalScope(this)) { service.deepSearch() }
        testScheduler.advanceUntilIdle()

        assertEquals(resumeFrom + DEEP_BATCHES * BATCH_SIZE, installations.previous(PREVIOUS).coinScanNextIndex)
        assertEquals(resumeFrom + DEEP_BATCHES * BATCH_SIZE, installations.previous(ANOTHER_PREVIOUS).coinScanNextIndex)
        assertEquals(BackupProgress.Deep.Completed, service.subscribeProgress().value())
    }

    private fun TestScope.start() {
        with(ComputationalScope(this)) { service.start() }
        testScheduler.advanceUntilIdle()
    }

    private fun Flow<BackupProgress>.value() = (this as MutableStateFlow<BackupProgress>).value

    private fun coinAt(key: CoinageKeyIndex) = Coin(
        derivationIndex = key,
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(0),
        isOnChain = true,
        accountId = byteArrayOf(key.item.toByte()).toDataByteArray(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private class InMemoryInstallations(private val current: CoinageInstallationId) : CoinageInstallationRepository {
        private val previous = linkedMapOf<CoinageInstallationId, PreviousInstallation>()

        fun previous(id: CoinageInstallationId) = previous.getValue(id)

        override suspend fun getOrCreateCurrent() = current

        override suspend fun addPrevious(installations: Collection<CoinageInstallationId>) {
            installations.filter { it != current && it !in previous }.forEach {
                previous[it] = PreviousInstallation(it, coinScanNextIndex = 0, voucherScanNextIndex = 0, initialScanCompleted = false)
            }
        }

        override suspend fun getPrevious() = previous.values.toList()

        override suspend fun updateCoinScanNextIndex(installation: CoinageInstallationId, nextIndex: Int) {
            previous[installation] = previous.getValue(installation).copy(coinScanNextIndex = nextIndex)
        }

        override suspend fun updateVoucherScanNextIndex(installation: CoinageInstallationId, nextIndex: Int) {
            previous[installation] = previous.getValue(installation).copy(voucherScanNextIndex = nextIndex)
        }

        override suspend fun markInitialScanCompleted(installation: CoinageInstallationId) {
            previous[installation] = previous.getValue(installation).copy(initialScanCompleted = true)
        }
    }

    private class InMemoryFlag : DeepRecoveryCompletedStorage {
        private val value = MutableStateFlow<Boolean?>(null)

        override fun valueFlow(): Flow<Boolean?> = value

        override suspend fun getValue() = value.value

        override suspend fun saveValue(value: Boolean) {
            this.value.value = value
        }

        override suspend fun removeValue() {
            value.value = null
        }

        override suspend fun requireValue() = value.value ?: false
    }

    private companion object {
        const val BATCH_SIZE = 500
        const val EMPTY_BATCHES = 4
        const val DEEP_BATCHES = 10

        val PREVIOUS = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x01 }.toDataByteArray())
        val ANOTHER_PREVIOUS = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x02 }.toDataByteArray())
        val CONTRACT = ByteArray(20).toDataByteArray()
    }
}
