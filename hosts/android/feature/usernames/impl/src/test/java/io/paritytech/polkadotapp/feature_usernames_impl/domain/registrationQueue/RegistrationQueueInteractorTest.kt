package io.paritytech.polkadotapp.feature_usernames_impl.domain.registrationQueue

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.QueueRateLimitedException
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.RegistrationQueueStatus
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class RegistrationQueueInteractorTest {
    private val username = Username.fromFullValue("alice.07")

    private val usernameRepository: UsernameRepository = mock(UsernameRepository::class.java)

    @Suppress("UNCHECKED_CAST")
    private val localUsernameStorage: LocalUsernameStorage =
        mock(SingleValueStorage::class.java) as LocalUsernameStorage
    private val queuedClaimStorage: QueuedClaimStorage = mock(QueuedClaimStorage::class.java)

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

        verify(localUsernameStorage).saveValue(username)
        verify(queuedClaimStorage).removeValue()
    }

    @Test
    fun `not queued without pending claim clears pending and saves nothing`() = runTest {
        withNoQueuedClaim()
        withQueueStatuses(listOf(Result.success(RegistrationQueueStatus.NotQueued)))

        interactor.observeQueueProgress().toList()

        verify(localUsernameStorage, never()).saveValue(any())
        verify(queuedClaimStorage).removeValue()
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
        var stubbing = whenever(usernameRepository.getRegistrationQueueStatus())
        resultsInOrder.forEach { stubbing = stubbing.thenReturn(it) }
    }

    private suspend fun withQueuedClaim(username: Username) {
        whenever(queuedClaimStorage.getValue()).thenReturn(username)
    }

    private suspend fun withNoQueuedClaim() {
        whenever(queuedClaimStorage.getValue()).thenReturn(null)
    }

    private fun inQueue(position: Int): RegistrationQueueStatus.InQueue {
        return RegistrationQueueStatus.InQueue(position = position, group = 1, estimatedIterationsRemaining = 0)
    }
}
