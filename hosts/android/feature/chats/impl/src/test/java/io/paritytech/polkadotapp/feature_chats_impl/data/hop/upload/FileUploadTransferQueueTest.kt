package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.net.Uri
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.NODE_A
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.NODE_B
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.NODE_C
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryDecision
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.thenThrowUnsafe
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

class FileUploadTransferQueueTest {
    private val uploadConcurrency: UploadConcurrency = mock(UploadConcurrency::class.java)
    private val repository: FileUploadRepository = mock(FileUploadRepository::class.java)
    private val hopNodeUrlProvider: HopNodeUrlProvider = mock(HopNodeUrlProvider::class.java)
    private val fileUri: Uri = mock(Uri::class.java)

    @Test
    fun `the upload queue allows the uploads in flight that UploadConcurrency gives when it is created`() {
        withUploadSlots(uploadConcurrency, ALLOWED_UPLOADS)

        assertEquals(ALLOWED_UPLOADS, queue().maxInFlight)
    }

    @Test
    fun `the repository releases in-progress uploads when the queue is released`() = runBlocking<Unit> {
        queue().releaseInProgress()

        verifyInProgressReleased()
    }

    @Test
    fun `a retry stays on its node when the upload failed once there`() = runBlocking<Unit> {
        queue().scheduleRetry(fileUpload(fileUri, nodeUrl = NODE_A), retryAfter(failures = 1))

        verifyRetryScheduledOn(NODE_A, failures = 1)
        verifyNeverAskedForAnotherNode()
    }

    @Test
    fun `a retry moves to the picked node when the upload failed twice on its node`() = runBlocking<Unit> {
        withRetryNode(failed = NODE_A, picked = NODE_B)

        queue().scheduleRetry(fileUpload(fileUri, nodeUrl = NODE_A, attemptCount = 1), retryAfter(failures = 2))

        verifyRetryScheduledOn(NODE_B, failures = 2)
    }

    @Test
    fun `a retry stays on the node it moved to when the upload failed once there`() = runBlocking<Unit> {
        queue().scheduleRetry(fileUpload(fileUri, nodeUrl = NODE_B, attemptCount = 2), retryAfter(failures = 3))

        verifyRetryScheduledOn(NODE_B, failures = 3)
        verifyNeverAskedForAnotherNode()
    }

    @Test
    fun `a retry moves again when the upload failed twice on the node it moved to`() = runBlocking<Unit> {
        withRetryNode(failed = NODE_B, picked = NODE_C)

        queue().scheduleRetry(fileUpload(fileUri, nodeUrl = NODE_B, attemptCount = 3), retryAfter(failures = 4))

        verifyRetryScheduledOn(NODE_C, failures = 4)
    }

    @Test
    fun `a retry stays on its node when no other node can be picked`() = runBlocking<Unit> {
        withNoRetryNode(failed = NODE_A)

        queue().scheduleRetry(fileUpload(fileUri, nodeUrl = NODE_A, attemptCount = 1), retryAfter(failures = 2))

        verifyRetryScheduledOn(NODE_A, failures = 2)
    }

    private fun queue() = FileUploadTransferQueue(
        repository = repository,
        uploader = mock(HopFileUploader::class.java),
        hopNodeUrlProvider = hopNodeUrlProvider,
        uploadConcurrency = uploadConcurrency
    )

    private suspend fun withRetryNode(failed: String, picked: String) {
        whenever(hopNodeUrlProvider.pickForRetry(failed)).thenReturn(picked)
    }

    private suspend fun withNoRetryNode(failed: String) {
        whenever(hopNodeUrlProvider.pickForRetry(failed)).thenThrowUnsafe(IllegalStateException("No Hop node URLs available"))
    }

    private fun retryAfter(failures: Int) = RetryDecision.Reschedule(
        attemptCount = failures,
        firstFailureAt = FIRST_FAILURE_AT,
        nextAttemptAt = NEXT_ATTEMPT_AT
    )

    private suspend fun verifyRetryScheduledOn(nodeUrl: String, failures: Int) {
        verify(repository).scheduleRetry(MESSAGE_ID, nodeUrl, failures, FIRST_FAILURE_AT, NEXT_ATTEMPT_AT)
    }

    private suspend fun verifyInProgressReleased() {
        verify(repository).releaseInProgress()
    }

    private suspend fun verifyNeverAskedForAnotherNode() {
        verify(hopNodeUrlProvider, never()).pickForRetry(any())
    }

    private companion object {
        const val ALLOWED_UPLOADS = 6
        val FIRST_FAILURE_AT = Instant.fromEpochMilliseconds(0)
        val NEXT_ATTEMPT_AT = FIRST_FAILURE_AT + 10.seconds
    }
}
