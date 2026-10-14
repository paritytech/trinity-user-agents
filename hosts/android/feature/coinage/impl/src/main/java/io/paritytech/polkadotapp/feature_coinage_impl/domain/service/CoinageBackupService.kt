package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.measureExecution
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.PreviousInstallation
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.DeepRecoveryCompletedStorage
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.logId
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

// Recovers balance held under the subtrees of this seed's previous installations.
//
// Only ever reads them: a coin found here is spendable, but new keys are allocated in the current installation
// alone, so nothing found here can move the index this installation hands out next.
class RealCoinageBackupService @Inject constructor(
    private val installationRepository: CoinageInstallationRepository,
    private val dataStoreConfigProvider: AccountDataStoreConfigProvider,
    private val dataStoreRepository: AccountDataStoreRepository,
    private val assetScanner: InstallationAssetScanner,
    private val coinsRepository: CoinRepository,
    private val voucherRepository: VoucherRepository,
    private val deepRecoveryCompletedStorage: DeepRecoveryCompletedStorage,
) : CoinageBackupService {
    private val progress = MutableStateFlow<BackupProgress>(BackupProgress.Unknown)

    override fun subscribeProgress(): Flow<BackupProgress> = progress

    context(scope: ComputationalScope)
    override fun start() {
        scope.launch { recoverNewInstallations() }
    }

    context(scope: ComputationalScope)
    override fun deepSearch() {
        scope.launch {
            if (progress.value.isInProgress()) return@launch

            progress.value = BackupProgress.Deep.Syncing

            val previous = installationRepository.getPrevious()
            val recovered = previous.map { deepScan(it) }
            coinageLogI("Deep recovery finished: ${recovered.describe()} across ${previous.size} installation(s)")

            progress.value = BackupProgress.Deep.Completed
        }
    }

    context(scope: ComputationalScope)
    override fun markAsCompleted() {
        scope.launch {
            if (progress.value.isInProgress()) return@launch

            deepRecoveryCompletedStorage.saveValue(true)
            progress.value = BackupProgress.Completed
        }
    }

    private suspend fun recoverNewInstallations() = measureExecution("Recovering previous installations") {
        // A failed read only delays discovery: installations found on an earlier launch are still scanned.
        dataStoreConfigProvider.contractAddress()
            .flatMap { contract -> dataStoreRepository.fetchRegisteredInstallations(contract, at = null) }
            .onSuccess { registered ->
                coinageLogI("Recovery: contract lists ${registered.size} installation(s) for this seed")
                installationRepository.addPrevious(registered)
            }
            .onFailure { coinageLogW("Recovery: could not read registered installations, scanning the ones already known: ${it.message}") }

        val previous = installationRepository.getPrevious()
        val pending = previous.filterNot { it.initialScanCompleted }

        if (pending.isEmpty()) {
            coinageLogI("Recovery: nothing new to scan, ${previous.size} previous installation(s) already scanned")
        } else {
            coinageLogI("Recovery: scanning ${pending.size} of ${previous.size} previous installation(s)")
            progress.value = BackupProgress.Initial.Syncing

            val recovered = pending.map { initialScan(it) }
            coinageLogI("Recovery finished: ${recovered.describe()} across ${pending.size} installation(s)")

            // Newly found balance is worth another look, even if the last one was acknowledged.
            deepRecoveryCompletedStorage.saveValue(false)
        }

        progress.value = when {
            deepRecoveryCompletedStorage.requireValue() -> BackupProgress.Completed
            previous.isEmpty() -> BackupProgress.Completed
            else -> BackupProgress.Initial.Completed
        }
    }

    private suspend fun initialScan(installation: PreviousInstallation): RecoveredAssets = coroutineScope {
        val coins = async { gapScanCoins(installation.id, installation.coinScanNextIndex, ScanLimit.UntilGap) }
        val vouchers = async { gapScanVouchers(installation.id, installation.voucherScanNextIndex, ScanLimit.UntilGap) }

        val recovered = RecoveredAssets(coins = coins.await(), vouchers = vouchers.await())
        coinageLogI("Recovery: installation=${installation.id.logId()} ${recovered.describe()}")

        if (recovered.isComplete) installationRepository.markInitialScanCompleted(installation.id)

        recovered
    }

    private suspend fun deepScan(installation: PreviousInstallation): RecoveredAssets = coroutineScope {
        val coins = async { gapScanCoins(installation.id, installation.coinScanNextIndex, ScanLimit.Batches(DEEP_SEARCH_BATCH_COUNT)) }
        val vouchers = async { gapScanVouchers(installation.id, installation.voucherScanNextIndex, ScanLimit.Batches(DEEP_SEARCH_BATCH_COUNT)) }

        RecoveredAssets(coins = coins.await(), vouchers = vouchers.await())
            .also { coinageLogI("Deep recovery: installation=${installation.id.logId()} ${it.describe()}") }
    }

    private suspend fun gapScanCoins(installation: CoinageInstallationId, startIndex: Int, limit: ScanLimit): Result<Int> {
        return gapScan(startIndex, limit) { batchStart ->
            assetScanner.scanCoins(installation, batchStart, BATCH_SIZE)
                .onSuccess { coinsRepository.saveAll(it) }
                .map { it.size }
        }
            .onSuccess { installationRepository.updateCoinScanNextIndex(installation, it.nextIndex) }
            .onFailure { coinageLogE("Recovery: coin scan of installation=${installation.logId()} failed", it) }
            .map { it.found }
    }

    private suspend fun gapScanVouchers(installation: CoinageInstallationId, startIndex: Int, limit: ScanLimit): Result<Int> {
        return gapScan(startIndex, limit) { batchStart ->
            assetScanner.scanVouchers(installation, batchStart, BATCH_SIZE)
                .onSuccess { voucherRepository.saveAll(it) }
                .map { it.size }
        }
            .onSuccess { installationRepository.updateVoucherScanNextIndex(installation, it.nextIndex) }
            .onFailure { coinageLogE("Recovery: voucher scan of installation=${installation.logId()} failed", it) }
            .map { it.found }
    }

    private suspend fun gapScan(
        startIndex: Int,
        limit: ScanLimit,
        scanBatch: suspend (batchStart: Int) -> Result<Int>,
    ): Result<GapScanResult> {
        var nextIndex = startIndex
        var batches = 0
        var emptyBatchesInARow = 0
        var found = 0

        while (!limit.isReached(batches, emptyBatchesInARow)) {
            val foundInBatch = scanBatch(nextIndex).getOrElse { return Result.failure(it) }

            found += foundInBatch
            emptyBatchesInARow = if (foundInBatch > 0) 0 else emptyBatchesInARow + 1
            batches += 1
            nextIndex += BATCH_SIZE
        }

        return Result.success(GapScanResult(nextIndex = nextIndex, found = found))
    }

    private class GapScanResult(val nextIndex: Int, val found: Int)

    // A failed scan counts nothing and leaves the installation for the next launch to finish.
    private class RecoveredAssets(val coins: Result<Int>, val vouchers: Result<Int>) {
        val isComplete: Boolean
            get() = coins.isSuccess && vouchers.isSuccess

        fun describe(): String = "coins=${coins.describeCount()} vouchers=${vouchers.describeCount()}"

        private fun Result<Int>.describeCount(): String = getOrNull()?.toString() ?: "failed"
    }

    private fun List<RecoveredAssets>.describe(): String {
        val coins = sumOf { it.coins.getOrDefault(0) }
        val vouchers = sumOf { it.vouchers.getOrDefault(0) }
        val failed = count { !it.isComplete }

        return "$coins coin(s) and $vouchers voucher(s) recovered" + if (failed > 0) ", $failed installation(s) incomplete" else ""
    }

    private sealed interface ScanLimit {
        fun isReached(batches: Int, emptyBatchesInARow: Int): Boolean

        data object UntilGap : ScanLimit {
            override fun isReached(batches: Int, emptyBatchesInARow: Int) = emptyBatchesInARow >= EMPTY_BATCH_COUNT
        }

        data class Batches(val count: Int) : ScanLimit {
            override fun isReached(batches: Int, emptyBatchesInARow: Int) = batches >= count
        }
    }

    private companion object {
        const val BATCH_SIZE = 500
        const val EMPTY_BATCH_COUNT = 4
        const val DEEP_SEARCH_BATCH_COUNT = 10
    }
}
