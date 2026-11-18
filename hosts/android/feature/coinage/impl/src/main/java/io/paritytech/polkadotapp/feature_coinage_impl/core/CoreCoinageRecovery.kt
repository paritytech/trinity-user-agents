package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.DeepRecoveryCompletedStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.launch
import uniffi.truapi.Event
import uniffi.truapi.HandOverProgress
import uniffi.truapi.NativeEventObserver
import uniffi.truapi.NativeHandOverObserver
import uniffi.truapi.Recoverability
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Recovery as the wallet screens show it. The core reconstructs the wallet on its own when that is due; what is
 * shown here is the scan of a wallet the app handed over, and the deep search the user asks for.
 */
@Singleton
class CoreCoinageBackupService @Inject constructor(
    private val core: CoinageCore,
    private val completedStorage: DeepRecoveryCompletedStorage,
) : CoinageBackupService {
    private val searched = MutableStateFlow<BackupProgress?>(null)

    override fun subscribeProgress(): Flow<BackupProgress> =
        combine(handOverProgress(), searched, completedStorage.valueFlow()) { handOver, search, acknowledged ->
            when {
                search != null && (search.isInProgress() || acknowledged != true) -> search
                acknowledged == true -> BackupProgress.Completed
                handOver == HandOverProgress.SCANNING -> BackupProgress.Initial.Syncing
                handOver == HandOverProgress.DONE -> BackupProgress.Initial.Completed
                else -> BackupProgress.Completed
            }
        }.distinctUntilChanged()

    context(scope: ComputationalScope)
    override fun start() = Unit

    context(scope: ComputationalScope)
    override fun deepSearch() {
        if (searched.value?.isInProgress() == true) return
        searched.value = BackupProgress.Deep.Syncing
        scope.launch {
            completedStorage.saveValue(false)
            val id = CoinageIds.random()
            core.followOperation(id) { coinage -> coinage.reconstruct(id, deep = true) }.collect()
            searched.value = BackupProgress.Deep.Completed
        }
    }

    context(scope: ComputationalScope)
    override fun markAsCompleted() {
        if (searched.value?.isInProgress() == true) return
        scope.launch {
            completedStorage.saveValue(true)
            searched.value = null
        }
    }

    private fun handOverProgress(): Flow<HandOverProgress> =
        core.observe { coinage, push ->
            coinage.observeHandOver(
                object : NativeHandOverObserver {
                    override fun onProgress(progress: HandOverProgress) = push(progress)
                }
            )
        }
}

class CoreCoinageAccountBackupObserver @Inject constructor(
    private val core: CoinageCore,
    private val config: CoreCoinageConfig,
) : CoinageAccountBackupObserver {
    override fun subscribeStatus(): Flow<CoinageAccountBackupStatus> = flow {
        val registryConfigured = config.registryContract() != null
        emitAll(
            recoverability().map { recoverability ->
                when {
                    recoverability is Recoverability.Registered -> CoinageAccountBackupStatus.Completed
                    registryConfigured -> CoinageAccountBackupStatus.Registering
                    else -> CoinageAccountBackupStatus.Delayed
                }
            }
        )
    }.distinctUntilChanged()

    override fun subscribeAllocationWaiting(): Flow<Boolean> = flow {
        val registryConfigured = config.registryContract() != null
        emitAll(recoverability().map { registryConfigured && it is Recoverability.Pending })
    }.distinctUntilChanged()

    /** The current installation's recoverability, read again whenever the core reports it may have changed. */
    private fun recoverability(): Flow<Recoverability> = core.observe<Event> { coinage, push ->
        coinage.observeEvents(
            object : NativeEventObserver {
                override fun onEvent(event: Event) = push(event)
            }
        )
    }
        .mapNotNull { event ->
            when (event) {
                is Event.Recoverability -> event.v1
                is Event.Resynced -> core.use { it.recoverable() }.getOrNull()
                else -> null
            }
        }
        .onStart { core.use { it.recoverable() }.onSuccess { emit(it) } }
}
