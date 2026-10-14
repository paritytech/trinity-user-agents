package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableSubmission
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.RegistrationScope
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CoinageInstallationRegistrarTest {
    private val engine = FakeEngine()
    private val attempts = mutableListOf<InstallationRegistrationTarget>()
    private var failNextAttempts = 0
    private var attemptGate: CompletableDeferred<Unit>? = null
    private val onboarded = MutableStateFlow(true)

    private val config = mockk<AccountDataStoreConfigProvider> {
        coEvery { contractAddress() } returns Result.success(CONTRACT)
    }

    // An accepted attempt shows up as a fresh PENDING row in its target's group; a gate holds it before it commits.
    private val submitter = mockk<InstallationRegistrationSubmitter> {
        coEvery { submitAttempt(any()) } coAnswers {
            val target = firstArg<InstallationRegistrationTarget>()
            attempts += target
            attemptGate?.await()

            if (failNextAttempts > 0) {
                failNextAttempts--
                Result.failure(IllegalStateException("not enough PGAS yet"))
            } else {
                val id = DurableTxId(100L + attempts.size)
                val group = target.registrationGroup()
                engine.set(engine.current(group) + DurableTxState(id, PENDING), group)
                Result.success(id)
            }
        }
    }

    private val onboardingStatus = mockk<ObserveAccountOnboardingStatusUseCase> {
        every { this@mockk.invoke() } returns onboarded.map { isOnboarded ->
            mockk<AccountOnboardingStatus> { every { this@mockk.isOnboarded } returns isOnboarded }
        }
    }

    @Test
    fun `a registration already final submits nothing and completes`() = registrarTest {
        engine.set(listOf(state(1, FINALIZED_SUCCESS)))

        start()

        assertEquals(0, attempts.size)
        assertEquals(CoinageAccountBackupStatus.Completed, status())
    }

    @Test
    fun `nothing is attempted before the account is onboarded`() = registrarTest {
        onboarded.value = false

        start()
        assertEquals(0, attempts.size)
        assertEquals(CoinageAccountBackupStatus.Registering, status())

        onboarded.value = true
        runCurrent()
        assertEquals(listOf(TARGET), attempts)
    }

    @Test
    fun `nothing registered yet submits exactly one attempt for this installation`() = registrarTest {
        start()

        assertEquals(listOf(TARGET), attempts)
    }

    @Test
    fun `a live attempt is never joined by a second one`() = registrarTest {
        engine.set(listOf(state(1, PENDING)))

        start()
        engine.set(listOf(state(1, PENDING_SUCCESS)))
        runCurrent()

        assertEquals(0, attempts.size)
    }

    @Test
    fun `identical group re-emissions do not restart an attempt in flight`() = registrarTest {
        val gate = CompletableDeferred<Unit>()
        attemptGate = gate

        start()
        engine.reEmit()
        runCurrent()
        engine.reEmit()
        runCurrent()
        gate.complete(Unit)
        runCurrent()

        assertEquals(1, attempts.size)
    }

    @Test
    fun `starting and observing more than once still runs one registration`() = registrarTest {
        val gate = CompletableDeferred<Unit>()
        attemptGate = gate

        start()
        start()
        backgroundScope.launchCollecting()
        runCurrent()
        gate.complete(Unit)
        runCurrent()

        assertEquals(1, attempts.size)
    }

    @Test
    fun `failures from earlier runs do not delay this run's first attempt`() = registrarTest {
        engine.set(listOf(state(1, FAILURE), state(2, FAILURE), state(3, FAILURE)))

        start()

        assertEquals(1, attempts.size)
    }

    @Test
    fun `an attempt that fails on chain is followed by exactly one more after a backoff`() = registrarTest {
        start()
        assertEquals(1, attempts.size)

        engine.set(engine.current().map { it.copy(status = FAILURE) })
        runCurrent()
        assertEquals(1, attempts.size)

        advanceTimeBy(INITIAL_BACKOFF_MS + 1)
        assertEquals(2, attempts.size)

        advanceTimeBy(MAX_BACKOFF_MS)
        assertEquals(2, attempts.size)
    }

    @Test
    fun `an attempt that could not be submitted is retried`() = registrarTest {
        failNextAttempts = 1

        start()
        assertEquals(1, attempts.size)

        advanceTimeBy(INITIAL_BACKOFF_MS + 1)
        assertEquals(2, attempts.size)
    }

    @Test
    fun `a changed contract starts a registration of its own`() = registrarTest {
        engine.set(listOf(state(1, FINALIZED_SUCCESS)), group = TARGET.registrationGroup())
        coEvery { config.contractAddress() } returns Result.success(OTHER_CONTRACT)

        start()

        assertEquals(listOf(InstallationRegistrationTarget(OTHER_CONTRACT, TEST_INSTALLATION)), attempts)
        assertEquals(CoinageAccountBackupStatus.Registering, status())
    }

    @Test
    fun `a contract address not yet available is waited for`() = registrarTest {
        coEvery { config.contractAddress() } returns Result.failure(IllegalStateException("remote config not synced"))

        start()
        assertEquals(0, attempts.size)

        coEvery { config.contractAddress() } returns Result.success(CONTRACT)
        advanceTimeBy(INITIAL_BACKOFF_MS + 1)

        assertEquals(listOf(TARGET), attempts)
    }

    @Test
    fun `a contract address that never arrives reads as delayed`() = registrarTest {
        coEvery { config.contractAddress() } returns Result.failure(IllegalStateException("remote config not synced"))

        start()
        advanceTimeBy(EXPECTED_REGISTRATION_MS + 1)

        assertEquals(CoinageAccountBackupStatus.Delayed, status())
    }

    @Test
    fun `a registration not final within the expected time reads as delayed`() = registrarTest {
        engine.set(listOf(state(1, PENDING)))
        start()

        advanceTimeBy(EXPECTED_REGISTRATION_MS - 1)
        assertEquals(CoinageAccountBackupStatus.Registering, status())

        advanceTimeBy(2)
        assertEquals(CoinageAccountBackupStatus.Delayed, status())
    }

    @Test
    fun `finality ends the run and clears the warning`() = registrarTest {
        engine.set(listOf(state(1, PENDING)))
        start()
        advanceTimeBy(EXPECTED_REGISTRATION_MS + 1)

        engine.set(listOf(state(1, FINALIZED_SUCCESS)))
        runCurrent()

        assertEquals(CoinageAccountBackupStatus.Completed, status())
    }

    @Test
    fun `recovery is started so attempts left by a previous process get decided`() = registrarTest {
        start()

        assertEquals(1, engine.recoveryStarts)
    }

    private fun registrarTest(body: suspend RegistrarScope.() -> Unit) = runTest {
        val registrar = CoinageInstallationRegistrar(
            observeAccountOnboardingStatusUseCase = onboardingStatus,
            installationRepository = mockk<CoinageInstallationRepository> { coEvery { getOrCreateCurrent() } returns TEST_INSTALLATION },
            configProvider = config,
            durableTransactionService = engine,
            submitter = submitter,
            chainConnectionRefCounter = mockk<ChainConnectionRefCounter>(relaxed = true),
            knownChains = KnownChains(people = "people", assetHub = "asset-hub", bulletIn = "bullet-in", hydration = null),
            dispatchers = testDispatchers(),
        )

        try {
            RegistrarScope(this, registrar).body()
        } finally {
            registrar.cancel()
        }
    }

    private class RegistrarScope(private val test: TestScope, private val registrar: CoinageInstallationRegistrar) {
        val backgroundScope get() = test.backgroundScope

        fun start() {
            registrar.start()
            test.runCurrent()
        }

        fun runCurrent() = test.runCurrent()

        fun advanceTimeBy(millis: Long) = test.advanceTimeBy(millis)

        suspend fun status() = registrar.subscribeStatus().first()

        fun CoroutineScope.launchCollecting() = registrar.subscribeStatus().launchIn(this)
    }

    private fun state(id: Long, status: DurableTxStatus) = DurableTxState(DurableTxId(id), status)

    /**
     * Emits like a Room flow: every write re-emits, identical or not, and a subscriber first sees the current rows.
     */
    private class FakeEngine : DurableTransactionService {
        private val groups = mutableMapOf<OperationGroupId, List<DurableTxState>>()
        private val updates = MutableSharedFlow<Pair<OperationGroupId, List<DurableTxState>>>(extraBufferCapacity = 64)
        var recoveryStarts = 0
            private set

        fun current(group: OperationGroupId = TARGET.registrationGroup()) = groups[group].orEmpty()

        fun set(states: List<DurableTxState>, group: OperationGroupId = TARGET.registrationGroup()) {
            groups[group] = states
            updates.tryEmit(group to states)
        }

        fun reEmit(group: OperationGroupId = TARGET.registrationGroup()) {
            updates.tryEmit(group to current(group))
        }

        override fun subscribeGroupStates(domain: TxDomainId, groupId: OperationGroupId): Flow<List<DurableTxState>> {
            require(domain == COINAGE_INSTALLATION_DOMAIN)
            return flow {
                emit(current(groupId))
                emitAll(updates.filter { it.first == groupId }.map { it.second })
            }
        }

        override suspend fun getGroupStates(domain: TxDomainId, groupId: OperationGroupId) = Result.success(current(groupId))

        override fun startRecovery() {
            recoveryStarts++
        }

        override suspend fun submit(
            domain: TxDomainId,
            extrinsic: EnrichedSendableExtrinsic,
            groupId: OperationGroupId?,
            onRegister: suspend RegistrationScope.(DurableTxId) -> Unit,
        ): Result<DurableTxId> = error("the registrar submits through its submitter")

        override suspend fun submitAll(
            domain: TxDomainId,
            submissions: List<DurableSubmission>,
            groupId: OperationGroupId,
            onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
        ): Result<List<DurableTxId>> = error("not used")

        override suspend fun schedule(
            domain: TxDomainId,
            groupId: OperationGroupId,
            policies: List<SubmissionPolicy>,
            onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
        ): Result<List<DurableTxId>> = error("not used")

        override suspend fun getStatus(id: DurableTxId): Result<DurableTxStatus> = error("not used")

        override fun subscribeStatus(id: DurableTxId): Flow<DurableTxStatus> = flowOf()
    }

    private companion object {
        const val INITIAL_BACKOFF_MS = 5_000L
        const val MAX_BACKOFF_MS = 5 * 60_000L
        const val EXPECTED_REGISTRATION_MS = 60_000L

        val CONTRACT = ByteArray(20) { 0x0c }.toDataByteArray()
        val OTHER_CONTRACT = ByteArray(20) { 0x0d }.toDataByteArray()
        val TARGET = InstallationRegistrationTarget(CONTRACT, TEST_INSTALLATION)
    }
}
