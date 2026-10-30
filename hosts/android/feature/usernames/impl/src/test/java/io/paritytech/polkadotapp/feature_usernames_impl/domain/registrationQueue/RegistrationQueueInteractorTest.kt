package io.paritytech.polkadotapp.feature_usernames_impl.domain.registrationQueue

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.QueueRateLimitedException
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.RegistrationQueueStatus
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class RegistrationQueueInteractorTest {
    private val username = Username.fromFullValue("alice.07")

    private val usernameRepository: UsernameRepository = mockk()

    private val localUsernameStorage: LocalUsernameStorage = mockk(relaxUnitFun = true)
    private val queuedClaimStorage: QueuedClaimStorage = mockk(relaxUnitFun = true)

    private val interactor = RegistrationQueueInteractor(
        usernameRepository = usernameRepository,
        localUsernameStorage = localUsernameStorage,
        queuedClaimStorage = queuedClaimStorage
    )

    @Test
    fun `emits waiting positions keeping initial position from first poll`() = runTest {
        withQueuedClaim(username)
        withQueueStatuses(
            listOf(
                Result.success(inQueue(position = 400)),
                Result.success(inQueue(position = 380)),
                Result.success(RegistrationQueueStatus.NotQueued)
            )
        )

        val emissions = interactor.observeQueueProgress().toList()

        assertEquals(
            listOf(
                RegistrationQueueProgress.Waiting(position = 400, initialPosition = 400),
                RegistrationQueueProgress.Waiting(position = 380, initialPosition = 400),
                RegistrationQueueProgress.Completed
            ),
            emissions
        )
    }

    @Test
    fun `not queued moves pending username to local storage and clears pending`() = runTest {
        withQueuedClaim(username)
        withQueueStatuses(listOf(Result.success(RegistrationQueueStatus.NotQueued)))

        interactor.observeQueueProgress().toList()

        coVerify(exactly = 1) { localUsernameStorage.saveValue(username) }
        coVerify(exactly = 1) { queuedClaimStorage.removeValue() }
    }

    @Test
    fun `not queued without pending claim clears pending and saves nothing`() = runTest {
        withNoQueuedClaim()
        withQueueStatuses(listOf(Result.success(RegistrationQueueStatus.NotQueued)))

        interactor.observeQueueProgress().toList()

        coVerify(exactly = 0) { localUsernameStorage.saveValue(any()) }
        coVerify(exactly = 1) { queuedClaimStorage.removeValue() }
    }

    @Test
    fun `successful polls are spaced by the poll interval`() = runTest {
        withQueuedClaim(username)
        withQueueStatuses(
            listOf(
                Result.success(inQueue(position = 3)),
                Result.success(inQueue(position = 2)),
                Result.success(RegistrationQueueStatus.NotQueued)
            )
        )

        interactor.observeQueueProgress().toList()

        val expected = RegistrationQueueInteractor.POLL_INTERVAL.inWholeMilliseconds * 2
        assertEquals(expected, currentTime)
    }

    @Test
    fun `rate limit honors Retry-After delay`() = runTest {
        withQueuedClaim(username)
        withQueueStatuses(
            listOf(
                Result.failure(QueueRateLimitedException(retryAfterSeconds = 30)),
                Result.success(RegistrationQueueStatus.NotQueued)
            )
        )

        interactor.observeQueueProgress().toList()

        assertEquals(30_000L, currentTime)
    }

    @Test
    fun `transport errors back off exponentially`() = runTest {
        withQueuedClaim(username)
        withQueueStatuses(
            listOf(
                Result.failure(RuntimeException("network down")),
                Result.failure(RuntimeException("network down")),
                Result.failure(RuntimeException("network down")),
                Result.success(RegistrationQueueStatus.NotQueued)
            )
        )

        interactor.observeQueueProgress().toList()

        assertEquals(70_000L, currentTime)
    }

    @Test
    fun `error keeps polling and recovers to waiting`() = runTest {
        withQueuedClaim(username)
        withQueueStatuses(
            listOf(
                Result.failure(RuntimeException("network down")),
                Result.success(inQueue(position = 5)),
                Result.success(RegistrationQueueStatus.NotQueued)
            )
        )

        val emissions = interactor.observeQueueProgress().toList()

        assertEquals(
            listOf(
                RegistrationQueueProgress.Waiting(position = 5, initialPosition = 5),
                RegistrationQueueProgress.Completed
            ),
            emissions
        )
    }

    private suspend fun withQueueStatuses(resultsInOrder: List<Result<RegistrationQueueStatus>>) {
        coEvery { usernameRepository.getRegistrationQueueStatus() } returnsMany resultsInOrder
    }

    private suspend fun withQueuedClaim(username: Username) {
        coEvery { queuedClaimStorage.getValue() } returns username
    }

    private suspend fun withNoQueuedClaim() {
        coEvery { queuedClaimStorage.getValue() } returns null
    }

    private fun inQueue(position: Int): RegistrationQueueStatus.InQueue {
        return RegistrationQueueStatus.InQueue(position = position, group = 1, estimatedIterationsRemaining = 0)
    }
}
