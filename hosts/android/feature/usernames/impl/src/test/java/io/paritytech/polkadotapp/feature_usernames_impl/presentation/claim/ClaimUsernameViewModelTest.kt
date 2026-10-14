package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor.UsernamesClaimInteractor
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameOutcome
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameAvailabilityState
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
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
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

private const val USERNAME = "alicent"

class ClaimUsernameViewModelTest {
    private val router: UsernamesRouter = mock(UsernamesRouter::class.java)
    private val interactor: UsernamesClaimInteractor = mock(UsernamesClaimInteractor::class.java)

    // viewModelScope runs on Dispatchers.Main, and the availability pipeline debounces by 300ms.
    private val testScope = TestScope()
    private val mainDispatcher = StandardTestDispatcher(testScope.testScheduler)

    @Before
    fun setUp() {
        Dispatchers.setMain(mainDispatcher)
        whenever(interactor.observeAccountOnboardingStatus()).thenReturn(flowOf(AccountOnboardingStatus.EMPTY))
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `availability failure surfaces an inline error instead of a dead button`() = runTest(mainDispatcher) {
        // The reported bug: the failure was swallowed with Timber.d, the state stayed NEUTRAL,
        // and the claim button sat disabled with nothing drawn on the field.
        whenever(interactor.checkUsernameAvailable(any()))
            .thenReturn(Result.failure(UsernameFlowError.VerificationUnavailable))

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
        whenever(interactor.areAccountsInitialized()).thenReturn(true)
        whenever(interactor.checkUsernameAvailable(any()))
            .thenReturn(Result.failure(UsernameFlowError.NoConnection))

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
        verify(interactor, never()).claimUsername(any(), any())
    }

    @Test
    fun `an unattestable device is sent to the claim unavailable screen`() = runTest(mainDispatcher) {
        givenClaimFails(UsernameFlowError.VerificationUnavailable)

        claimWith(createViewModel())

        verify(router).openClaimUnavailable()
        verify(router, never()).openIntegrityFailed()
    }

    @Test
    fun `a rejected device is sent to the integrity failed screen`() = runTest(mainDispatcher) {
        givenClaimFails(UsernameFlowError.VerificationRejected)

        claimWith(createViewModel())

        verify(router).openIntegrityFailed()
        verify(router, never()).openClaimUnavailable()
    }

    @Test
    fun `backing out shows no error at all`() = runTest(mainDispatcher) {
        whenever(interactor.checkUsernameAvailable(any()))
            .thenReturn(Result.failure(UsernameFlowError.Cancelled))

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
        verify(router, never()).openIntegrityFailed()
        verify(router, never()).openClaimUnavailable()
    }

    private suspend fun givenClaimFails(error: UsernameFlowError) {
        whenever(interactor.areAccountsInitialized()).thenReturn(true)
        whenever(interactor.checkUsernameAvailable(any()))
            .thenReturn(Result.success(UsernameAvailabilityState.Available(emptyList())))
        whenever(interactor.claimUsername(any(), any())).thenReturn(ClaimUsernameOutcome.Failed(error))
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
