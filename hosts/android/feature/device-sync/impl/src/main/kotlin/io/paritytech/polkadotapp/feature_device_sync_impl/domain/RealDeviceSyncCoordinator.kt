package io.paritytech.polkadotapp.feature_device_sync_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CollectionDiffer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.mapListNotNull
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_device_sync_api.domain.DeviceSyncCoordinator
import io.paritytech.polkadotapp.feature_device_sync_impl.data.storage.SyncUpdateIdProvider
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine.DeviceSyncEngine
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine.LocalSyncTrigger
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine.SyncEntityApplier
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine.SyncEntityCollector
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.session.DevicesSessionManager
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.session.OwnDeviceSession
import io.paritytech.polkadotapp.feature_sso_api.domain.OwnDevicesJournal
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelFactory
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/** Spawns a [DeviceSyncEngine] per [OwnDeviceSession] (disposes on removal). */
@Singleton
class RealDeviceSyncCoordinator @Inject constructor(
    private val devicesSessionManager: DevicesSessionManager,
    private val peerChannelFactory: PeerChannelFactory,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val ownDevicesJournal: OwnDevicesJournal,
    private val syncUpdateIdProvider: SyncUpdateIdProvider,
    private val collector: SyncEntityCollector,
    private val applier: SyncEntityApplier,
    private val localSyncTrigger: LocalSyncTrigger,
    dispatchers: CoroutineDispatchers,
) : DeviceSyncCoordinator, CoroutineScope {
    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val engines = MutableStateFlow<Map<AccountId, DeviceSyncEngine>>(emptyMap())

    override fun startSubscriptions() {
        Timber.i("DeviceSyncCoordinator: starting")
        devicesSessionManager.startSubscriptions()

        devicesSessionManager.observeSessions()
            .mapListNotNull { it.getIfSyncableOrNull() }
            .diffed()
            .onEach(::applyDiff)
            .launchIn(this)
    }

    private suspend fun applyDiff(diff: CollectionDiffer.Diff<OwnDeviceSession>) {
        val ourStatementAccountId = ourStatementAccountId()

        diff.added.forEach { session ->
            Timber.d("Device ${session.peer.name}-${session.peer.id} started sync")
            val engineScope = childScope(supervised = true)
            val engine = DeviceSyncEngine(
                ourStatementAccountId = ourStatementAccountId,
                session = session,
                peerChannelFactory = peerChannelFactory,
                ownDevicesJournal = ownDevicesJournal,
                updateIdProvider = syncUpdateIdProvider,
                collector = collector,
                applier = applier,
                localSyncTrigger = localSyncTrigger,
                scope = engineScope,
            )
            engine.start()
            engines.update { it + (session.peer.statementAccountId to engine) }
        }

        diff.removed.forEach { session ->
            val deviceId = session.peer.statementAccountId
            engines.value[deviceId]?.dispose()
            engines.update { it - deviceId }

            Timber.d("Device ${session.peer.name}-${session.peer.id} stopped sync")
        }
    }

    private suspend fun ourStatementAccountId(): AccountId {
        val walletAccount = accountRepository.getWalletAccount()
        return walletAccount.accountIdIn(chainRegistry.getChain(knownChains.people))
    }

    private fun OwnDeviceSession.getIfSyncableOrNull() = takeIf { peer.name == SYNCABLE_HOST_NAME }

    private companion object {
        private const val SYNCABLE_HOST_NAME = "Polkadot Desktop"
    }
}
