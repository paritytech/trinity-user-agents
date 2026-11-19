package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.shareIn
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

// Registers this installation in the AccountDataStore contract, once per process, and reports how it goes.
//
// The durable engine decides what each attempt did; the loop here only decides when another attempt is needed.
// It never submits while one is live: the oracle reads the same record for every attempt, so two in flight would
// both be credited with it.
@Singleton
class CoinageInstallationRegistrar @Inject constructor(
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase,
    private val installationRepository: CoinageInstallationRepository,
    private val configProvider: AccountDataStoreConfigProvider,
    private val durableTransactionService: DurableTransactionService,
    private val submitter: InstallationRegistrationSubmitter,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
    dispatchers: CoroutineDispatchers,
) : CoroutineScope by CoroutineScope(dispatchers.io + SupervisorJob()) {
    private val registration: SharedFlow<CoinageAccountBackupStatus> = flow {
        val installation = installationRepository.getOrCreateCurrent()
        val finalizedBefore = finalizedRegistrationOf(installation)
        val completedBefore = finalizedBefore != null

        emit(if (completedBefore) CoinageAccountBackupStatus.Completed else CoinageAccountBackupStatus.Registering)

        observeAccountOnboardingStatusUseCase().first { it.isOnboarded }

        // Overdue counts from here, so a contract address that never arrives is reported like any other delay.
        val completed = flow {
            emit(completedBefore)

            val target = awaitTarget(installation)
            if (target != finalizedBefore) emit(false)

            registerUntilFinalized(target)
            emit(true)
        }
        emitAll(combine(completed, overdueAfter(EXPECTED_REGISTRATION_TIME), ::statusOf))
    }
        .distinctUntilChanged()
        .onEach { status -> coinageLogI("Installation registration status=$status") }
        .shareIn(this, SharingStarted.Lazily, replay = 1)

    fun subscribeStatus(): Flow<CoinageAccountBackupStatus> = registration

    fun start() {
        registration.launchIn(this)
    }

    private suspend fun registerUntilFinalized(target: InstallationRegistrationTarget) {
        coinageLogI("Installation registration starting, ${target.logDescription()}")
        durableTransactionService.startRecovery()

        chainConnectionRefCounter.withConnectionEnabled(knownChains.assetHub, CONNECTION_LABEL) {
            var attempts = 0

            while (true) {
                val settled = awaitNoLiveAttempt(target)

                // An attempt finalized. Registration finished.
                if (settled.any { it.status == DurableTxStatus.FINALIZED_SUCCESS }) {
                    coinageLogI("Installation registration finalized after $attempts attempt(s) in this run, ${target.logDescription()}")
                    break
                }

                // Only attempts of this run count: failures an earlier launch left behind say nothing about now.
                if (attempts > 0) {
                    val backoff = backoffFor(attempts)
                    coinageLogW("Installation registration attempt $attempts did not land, retrying in $backoff")
                    delay(backoff)
                }

                attempts++
                coinageLogI("Installation registration attempt $attempts")
                submitter.submitAttempt(target)
                    .onFailure { coinageLogE("Installation registration attempt $attempts could not be submitted", it) }
            }
        }
    }

    // Subscribed afresh after every attempt, so the first emission is read after that attempt committed.
    private suspend fun awaitNoLiveAttempt(target: InstallationRegistrationTarget): List<DurableTxState> {
        return durableTransactionService.subscribeGroupStates(COINAGE_INSTALLATION_DOMAIN, target.registrationGroup())
            .first { states -> states.none { it.status.isLive } }
    }

    // Read before remote config syncs, so a registration that already finalized does not hold allocations until then.
    private suspend fun finalizedRegistrationOf(installation: CoinageInstallationId): InstallationRegistrationTarget? {
        val contract = configProvider.lastKnownContractAddress().getOrNull() ?: return null
        val target = InstallationRegistrationTarget(contract, installation)

        return durableTransactionService.getGroupStates(COINAGE_INSTALLATION_DOMAIN, target.registrationGroup()).fold(
            onSuccess = { states -> target.takeIf { states.any { it.status == DurableTxStatus.FINALIZED_SUCCESS } } },
            onFailure = {
                coinageLogW("Installation registration: could not read the earlier registration, checking it again")
                null
            },
        )
    }

    private suspend fun awaitTarget(installation: CoinageInstallationId): InstallationRegistrationTarget {
        while (true) {
            configProvider.contractAddress()
                .onFailure { coinageLogW("Installation registration: data store contract address is not available yet") }
                .onSuccess { contract -> return InstallationRegistrationTarget(contract, installation) }

            delay(INITIAL_BACKOFF)
        }
    }

    private fun overdueAfter(duration: Duration): Flow<Boolean> = flow {
        emit(false)
        delay(duration)
        emit(true)
    }

    private fun statusOf(completed: Boolean, overdue: Boolean): CoinageAccountBackupStatus = when {
        completed -> CoinageAccountBackupStatus.Completed
        overdue -> CoinageAccountBackupStatus.Delayed
        else -> CoinageAccountBackupStatus.Registering
    }

    private fun backoffFor(attempts: Int): Duration {
        val exponential = INITIAL_BACKOFF * (1 shl (attempts - 1).coerceAtMost(MAX_BACKOFF_DOUBLINGS))
        return exponential.coerceAtMost(MAX_BACKOFF)
    }

    private companion object {
        const val CONNECTION_LABEL = "CoinageInstallationRegistrar"
        const val MAX_BACKOFF_DOUBLINGS = 10

        val EXPECTED_REGISTRATION_TIME = 1.minutes
        val INITIAL_BACKOFF = 5.seconds
        val MAX_BACKOFF = 5.minutes
    }
}
