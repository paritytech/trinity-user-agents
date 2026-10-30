package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.database.dao.FileUploadDao
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import kotlin.time.Instant

class FileUploadRepositoryTest {
    private val dao: FileUploadDao = mock(FileUploadDao::class.java)

    private val repository = FileUploadRepository(dao, FakeTimeProvider { NOW_MILLIS })

    @Test
    fun `every upload-table write reaches the pass even when the pending ids are unchanged`() = runBlocking<Unit> {
        withPendingIdsInvalidated(listOf("a"), listOf("a"))

        val changes = repository.observeQueueChanges().toList()

        assertEquals(2, changes.size)
    }

    @Test
    fun `the claim is due by the injected clock when it takes the next upload`() = runBlocking<Unit> {
        repository.claimNextDue()

        verifyClaimedAt(NOW_MILLIS)
    }

    @Test
    fun `the dao releases in-progress uploads when the repository is released`() = runBlocking<Unit> {
        repository.releaseInProgress()

        verifyInProgressReleased()
    }

    @Test
    fun `a retry is stored on the given node when the repository schedules it`() = runBlocking<Unit> {
        repository.scheduleRetry(MESSAGE_ID, NODE_URL, ATTEMPTS, FIRST_FAILURE_AT, NEXT_ATTEMPT_AT)

        verifyRetryStored(MESSAGE_ID, NODE_URL, ATTEMPTS, FIRST_FAILURE_AT_MILLIS, NEXT_ATTEMPT_AT_MILLIS)
    }

    private fun withPendingIdsInvalidated(vararg pendingIdsPerInvalidation: List<String>) {
        whenever(dao.observePendingIds()).thenReturn(flowOf(*pendingIdsPerInvalidation))
    }

    private suspend fun verifyClaimedAt(nowMillis: Long) {
        verify(dao).claimNextDue(nowMillis)
    }

    private suspend fun verifyInProgressReleased() {
        verify(dao).releaseInProgress()
    }

    private suspend fun verifyRetryStored(messageId: String, nodeUrl: String, attempts: Int, firstFailureAt: Long, nextAttemptAt: Long) {
        verify(dao).scheduleRetry(messageId, nodeUrl, attempts, firstFailureAt, nextAttemptAt)
    }

    private companion object {
        const val NOW_MILLIS = 1_700_000_000_000L
        const val MESSAGE_ID = "message-a"
        const val NODE_URL = "wss://node-b"
        const val ATTEMPTS = 2
        const val FIRST_FAILURE_AT_MILLIS = 1_000L
        const val NEXT_ATTEMPT_AT_MILLIS = 21_000L
        val FIRST_FAILURE_AT = Instant.fromEpochMilliseconds(FIRST_FAILURE_AT_MILLIS)
        val NEXT_ATTEMPT_AT = Instant.fromEpochMilliseconds(NEXT_ATTEMPT_AT_MILLIS)
    }
}
