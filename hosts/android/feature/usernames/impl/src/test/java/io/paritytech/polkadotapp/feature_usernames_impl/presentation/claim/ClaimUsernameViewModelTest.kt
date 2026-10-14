package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor.UsernamesClaimInteractor
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameOutcome
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameAvailabilityState
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

private const val USERNAME = "alicent"

class ClaimUsernameViewModelTest {
    private val router: UsernamesRouter = mockk(relaxUnitFun = true)
    private val interactor: UsernamesClaimInteractor = mockk()

    // viewModelScope runs on Dispatchers.Main, and the availability pipeline debounces by 300ms.
    private val testScope = TestScope()
    private val mainDispatcher = StandardTestDispatcher(testScope.testScheduler)

    @Before
    fun setUp() {
        Dispatchers.setMain(mainDispatcher)
        every { interactor.observeAccountOnboardingStatus() } returns flowOf(AccountOnboardingStatus.EMPTY)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `availability failure surfaces an inline error instead of a dead button`() = runTest(mainDispatcher) {
        // The reported bug: the failure was swallowed with Timber.d, the state stayed NEUTRAL,
        // and the claim button sat disabled with nothing drawn on the field.
        coEvery { interactor.checkUsernameAvailable(any()) } returns
            Result.failure(UsernameFlowError.VerificationUnavailable)

        val viewModel = createViewModel()
        viewModel.onUsernameChanged(USERNAME)
        advanceUntilIdle()

        assertEquals(
            ClaimUsernameFieldState.Error(UsernameFlowError.VerificationUnavailable),
            viewModel.state.value.fieldState
        )
        assertEquals(false, viewModel.state.value.claimButtonEnabled)
    }

    @Test
    fun `a failed pre-claim recheck does not report the username as taken`() = runTest(mainDispatcher) {
        // Item A: the re-check used .getOrDefault(false), so a network blip rendered as "Taken".
        coEvery { interactor.areAccountsInitialized() } returns true
        coEvery { interactor.checkUsernameAvailable(any()) } returns Result.failure(UsernameFlowError.NoConnection)

        val viewModel = createViewModel()
        viewModel.onUsernameChanged(USERNAME)
        advanceUntilIdle()
        viewModel.onClaimClicked()
        advanceUntilIdle()

        assertEquals(
            ClaimUsernameFieldState.Error(UsernameFlowError.NoConnection),
            viewModel.state.value.fieldState
        )
        assertEquals(ClaimUsernameProgress.NONE, viewModel.state.value.progress)
        coVerify(exactly = 0) { interactor.claimUsername(any(), any()) }
    }

    @Test
    fun `an unattestable device is sent to the claim unavailable screen`() = runTest(mainDispatcher) {
        givenClaimFails(UsernameFlowError.VerificationUnavailable)

        claimWith(createViewModel())

        verify(exactly = 1) { router.openClaimUnavailable() }
        verify(exactly = 0) { router.openIntegrityFailed() }
    }

    @Test
    fun `a rejected device is sent to the integrity failed screen`() = runTest(mainDispatcher) {
        givenClaimFails(UsernameFlowError.VerificationRejected)

        claimWith(createViewModel())

        verify(exactly = 1) { router.openIntegrityFailed() }
        verify(exactly = 0) { router.openClaimUnavailable() }
    }

    @Test
    fun `backing out shows no error at all`() = runTest(mainDispatcher) {
        coEvery { interactor.checkUsernameAvailable(any()) } returns Result.failure(UsernameFlowError.Cancelled)

        val viewModel = createViewModel()
        viewModel.onUsernameChanged(USERNAME)
        advanceUntilIdle()

        assertEquals(ClaimUsernameFieldState.Neutral, viewModel.state.value.fieldState)
        assertEquals(ClaimUsernameProgress.NONE, viewModel.state.value.progress)
    }

    @Test
    fun `a transport failure at claim time stays on the field`() = runTest(mainDispatcher) {
        givenClaimFails(UsernameFlowError.NoConnection)

        val viewModel = createViewModel()
        claimWith(viewModel)

        assertEquals(
            ClaimUsernameFieldState.Error(UsernameFlowError.NoConnection),
            viewModel.state.value.fieldState
        )
        verify(exactly = 0) { router.openIntegrityFailed() }
        verify(exactly = 0) { router.openClaimUnavailable() }
    }

    private suspend fun givenClaimFails(error: UsernameFlowError) {
        coEvery { interactor.areAccountsInitialized() } returns true
        coEvery { interactor.checkUsernameAvailable(any()) } returns
            Result.success(UsernameAvailabilityState.Available(emptyList()))
        coEvery { interactor.claimUsername(any(), any()) } returns ClaimUsernameOutcome.Failed(error)
    }

    private fun createViewModel() = ClaimUsernameViewModel(
        router = router,
        interactor = interactor,
    )
}

private fun kotlinx.coroutines.test.TestScope.claimWith(viewModel: ClaimUsernameViewModel) {
    viewModel.onUsernameChanged(USERNAME)
    advanceUntilIdle()
    viewModel.onClaimClicked()
    advanceUntilIdle()
}
