package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

interface CurrentInstallationProvider {
    suspend fun awaitBackedUp(): Result<CoinageInstallationId>
}

@Singleton
class RealCurrentInstallationProvider @Inject constructor(
    private val registrar: CoinageInstallationRegistrar,
    private val installationRepository: CoinageInstallationRepository,
) : CurrentInstallationProvider, CoinageAccountBackupObserver {
    private val waitingCount = MutableStateFlow(0)

    override fun subscribeStatus(): Flow<CoinageAccountBackupStatus> = registrar.subscribeStatus()

    override fun subscribeAllocationWaiting(): Flow<Boolean> = waitingCount.map { it > 0 }.distinctUntilChanged()

    override suspend fun awaitBackedUp(): Result<CoinageInstallationId> = runCancellableCatching {
        val status = registrar.subscribeStatus()

        if (status.first() != CoinageAccountBackupStatus.Completed) {
            waitingCount.update { it + 1 }
            try {
                status.first { it == CoinageAccountBackupStatus.Completed }
            } finally {
                waitingCount.update { it - 1 }
            }
        }

        installationRepository.getOrCreateCurrent()
    }
}
