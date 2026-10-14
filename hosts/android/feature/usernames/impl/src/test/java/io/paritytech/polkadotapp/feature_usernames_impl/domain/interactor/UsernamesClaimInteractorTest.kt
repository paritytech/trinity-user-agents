package io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.newaccount.NewAccountStorage
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.TryRecoverFromBackupAndCreateAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.RecoverUsernameUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameOutcome
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameParams
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameAvailabilityState
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameClaimResult
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.CreateClaimParamsUseCase
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify

class UsernamesClaimInteractorTest {
    private val username = Username.fromFullValue("alice.07")
    private val claimParams: ClaimUsernameParams = mock(ClaimUsernameParams::class.java)

    private val newAccountStorage: NewAccountStorage = mock(NewAccountStorage::class.java)
    private val usernameRepository: UsernameRepository = mock(UsernameRepository::class.java)

    @Suppress("UNCHECKED_CAST")
    private val localUsernameStorage: LocalUsernameStorage =
        mock(SingleValueStorage::class.java) as LocalUsernameStorage
    private val queuedClaimStorage: QueuedClaimStorage = mock(QueuedClaimStorage::class.java)
    private val createClaimParamsUseCase: CreateClaimParamsUseCase = mock(CreateClaimParamsUseCase::class.java)
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase =
        mock(ObserveAccountOnboardingStatusUseCase::class.java)
    private val tryRecoverFromBackupAndCreateAccountUseCase: TryRecoverFromBackupAndCreateAccountUseCase =
        mock(TryRecoverFromBackupAndCreateAccountUseCase::class.java)
    private val accountRepository: AccountRepository = mock(AccountRepository::class.java)
    private val recoverUsernameUseCase: RecoverUsernameUseCase = mock(RecoverUsernameUseCase::class.java)

    private val interactor = RealUsernamesClaimInteractor(
        newAccountStorage = newAccountStorage,
        usernameRepository = usernameRepository,
        coroutineDispatchers = RealCoroutineDispatchers(),
        localUsernameStorage = localUsernameStorage,
        queuedClaimStorage = queuedClaimStorage,
        createClaimParamsUseCase = createClaimParamsUseCase,
        observeAccountOnboardingStatusUseCase = observeAccountOnboardingStatusUseCase,
        tryRecoverFromBackupAndCreateAccountUseCase = tryRecoverFromBackupAndCreateAccountUseCase,
        accountRepository = accountRepository,
        recoverUsernameUseCase = recoverUsernameUseCase
    )

    @Before
    fun setUp() = runTest {
        withClaimParamsCreated()
    }

    @Test
    fun `registered claim saves username locally and reports Claimed`() = runTest {
        withClaimResult(UsernameClaimResult.Registered(username))

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Claimed, outcome)
        verify(localUsernameStorage).saveValue(username)
        verify(queuedClaimStorage, never()).saveValue(any())
    }

    @Test
    fun `queued claim saves pending username and reports Queued`() = runTest {
        withClaimResult(UsernameClaimResult.Queued(username))

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Queued, outcome)
        verify(queuedClaimStorage).saveValue(username)
        verify(localUsernameStorage, never()).saveValue(any())
    }

    @Test
    fun `payment required claim saves nothing and reports PaymentRequired`() = runTest {
        withClaimResult(UsernameClaimResult.PaymentRequired)

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.PaymentRequired, outcome)
        verify(localUsernameStorage, never()).saveValue(any())
        verify(queuedClaimStorage, never()).saveValue(any())
    }

    @Test
    fun `claim failure reports Failed with a classified error`() = runTest {
        whenever(usernameRepository.claimUsername(eq(claimParams)))
            .thenReturn(Result.failure(BackendRequestError.NoConnection))

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Failed(UsernameFlowError.NoConnection), outcome)
    }

    @Test
    fun `an unclassifiable claim failure becomes Unknown`() = runTest {
        whenever(usernameRepository.claimUsername(eq(claimParams)))
            .thenReturn(Result.failure(RuntimeException("network down")))

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Failed(UsernameFlowError.Unknown), outcome)
    }

    @Test
    fun `a transient attestation failure is retried exactly once`() = runTest {
        // Play Integrity blips clear on the next challenge, so one more attempt is spent
        // before the user is told anything.
        whenever(usernameRepository.checkUsernameAvailable(eq(username)))
            .thenReturn(Result.failure(integrityFailure(IntegrityError.AttestationTransient)))
            .thenReturn(Result.success(UsernameAvailabilityState.Available(listOf("07"))))

        val result = interactor.checkUsernameAvailable(username)

        assertEquals(UsernameAvailabilityState.Available(listOf("07")), result.getOrThrow())
        verify(usernameRepository, times(2)).checkUsernameAvailable(eq(username))
    }

    @Test
    fun `a persistent transient failure surfaces after the single retry`() = runTest {
        whenever(usernameRepository.checkUsernameAvailable(eq(username)))
            .thenReturn(Result.failure(integrityFailure(IntegrityError.AttestationTransient)))

        val result = interactor.checkUsernameAvailable(username)

        assertEquals(UsernameFlowError.VerificationBusy, result.exceptionOrNull())
        verify(usernameRepository, times(2)).checkUsernameAvailable(eq(username))
    }

    @Test
    fun `a non-transient availability failure is not retried`() = runTest {
        whenever(usernameRepository.checkUsernameAvailable(eq(username)))
            .thenReturn(Result.failure(integrityFailure(IntegrityError.AttestationRejected)))

        val result = interactor.checkUsernameAvailable(username)

        assertEquals(UsernameFlowError.VerificationRejected, result.exceptionOrNull())
        verify(usernameRepository, times(1)).checkUsernameAvailable(eq(username))
    }

    private fun integrityFailure(error: IntegrityError): BackendRequestError =
        BackendRequestError.Auth(AuthError.Integrity(error))

    private suspend fun withClaimParamsCreated() {
        whenever(usernameRepository.getVerifier()).thenReturn(Result.success("attester"))
        whenever(createClaimParamsUseCase(eq(username), eq("attester"), eq("07")))
            .thenReturn(Result.success(claimParams))
    }

    private suspend fun withClaimResult(result: UsernameClaimResult) {
        whenever(usernameRepository.claimUsername(eq(claimParams))).thenReturn(Result.success(result))
    }
}
