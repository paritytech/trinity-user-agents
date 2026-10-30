package io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
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
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

class UsernamesClaimInteractorTest {
    private val username = Username.fromFullValue("alice.07")
    private val claimParams: ClaimUsernameParams = mockk()

    private val newAccountStorage: NewAccountStorage = mockk()
    private val usernameRepository: UsernameRepository = mockk()

    private val localUsernameStorage: LocalUsernameStorage = mockk(relaxUnitFun = true)
    private val queuedClaimStorage: QueuedClaimStorage = mockk(relaxUnitFun = true)
    private val createClaimParamsUseCase: CreateClaimParamsUseCase = mockk()
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase = mockk()
    private val tryRecoverFromBackupAndCreateAccountUseCase: TryRecoverFromBackupAndCreateAccountUseCase = mockk()
    private val accountRepository: AccountRepository = mockk()
    private val recoverUsernameUseCase: RecoverUsernameUseCase = mockk()

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
        coVerify(exactly = 1) { localUsernameStorage.saveValue(username) }
        coVerify(exactly = 0) { queuedClaimStorage.saveValue(any()) }
    }

    @Test
    fun `queued claim saves pending username and reports Queued`() = runTest {
        withClaimResult(UsernameClaimResult.Queued(username))

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Queued, outcome)
        coVerify(exactly = 1) { queuedClaimStorage.saveValue(username) }
        coVerify(exactly = 0) { localUsernameStorage.saveValue(any()) }
    }

    @Test
    fun `payment required claim saves nothing and reports PaymentRequired`() = runTest {
        withClaimResult(UsernameClaimResult.PaymentRequired)

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.PaymentRequired, outcome)
        coVerify(exactly = 0) { localUsernameStorage.saveValue(any()) }
        coVerify(exactly = 0) { queuedClaimStorage.saveValue(any()) }
    }

    @Test
    fun `claim failure reports Failed with a classified error`() = runTest {
        coEvery { usernameRepository.claimUsername(claimParams) } returns
            Result.failure(BackendRequestError.NoConnection)

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Failed(UsernameFlowError.NoConnection), outcome)
    }

    @Test
    fun `an unclassifiable claim failure becomes Unknown`() = runTest {
        coEvery { usernameRepository.claimUsername(claimParams) } returns
            Result.failure(RuntimeException("network down"))

        val outcome = interactor.claimUsername(username, preferredDigits = "07")

        assertEquals(ClaimUsernameOutcome.Failed(UsernameFlowError.Unknown), outcome)
    }

    @Test
    fun `a transient attestation failure is retried exactly once`() = runTest {
        // Play Integrity blips clear on the next challenge, so one more attempt is spent
        // before the user is told anything.
        coEvery { usernameRepository.checkUsernameAvailable(username) } returnsMany listOf(
            Result.failure(integrityFailure(IntegrityError.AttestationTransient)),
            Result.success(UsernameAvailabilityState.Available(listOf("07")))
        )

        val result = interactor.checkUsernameAvailable(username)

        assertEquals(UsernameAvailabilityState.Available(listOf("07")), result.getOrThrow())
        coVerify(exactly = 2) { usernameRepository.checkUsernameAvailable(username) }
    }

    @Test
    fun `a persistent transient failure surfaces after the single retry`() = runTest {
        coEvery { usernameRepository.checkUsernameAvailable(username) } returns
            Result.failure(integrityFailure(IntegrityError.AttestationTransient))

        val result = interactor.checkUsernameAvailable(username)

        assertEquals(UsernameFlowError.VerificationBusy, result.exceptionOrNull())
        coVerify(exactly = 2) { usernameRepository.checkUsernameAvailable(username) }
    }

    @Test
    fun `a non-transient availability failure is not retried`() = runTest {
        coEvery { usernameRepository.checkUsernameAvailable(username) } returns
            Result.failure(integrityFailure(IntegrityError.AttestationRejected))

        val result = interactor.checkUsernameAvailable(username)

        assertEquals(UsernameFlowError.VerificationRejected, result.exceptionOrNull())
        coVerify(exactly = 1) { usernameRepository.checkUsernameAvailable(username) }
    }

    private fun integrityFailure(error: IntegrityError): BackendRequestError =
        BackendRequestError.Auth(AuthError.Integrity(error))

    private suspend fun withClaimParamsCreated() {
        coEvery { usernameRepository.getVerifier() } returns Result.success("attester")
        coEvery { createClaimParamsUseCase(username, "attester", "07") } returns Result.success(claimParams)
    }

    private suspend fun withClaimResult(result: UsernameClaimResult) {
        coEvery { usernameRepository.claimUsername(claimParams) } returns Result.success(result)
    }
}
