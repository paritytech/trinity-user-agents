@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.ChatMessageAttachmentUpdater
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.NODE_A
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction.anyHopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.patternBytes
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.HopTransferRetryPolicy
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.MAX_IN_FLIGHT
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryableTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueueProcessor
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.assertFailureOf
import io.paritytech.polkadotapp.test_shared.capture
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.testDispatchers
import io.paritytech.polkadotapp.test_shared.thenSuspend
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.ArgumentCaptor
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import java.io.ByteArrayInputStream
import java.io.FileNotFoundException
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

class HopFileUploaderBoundTest {
    private val fileUri: Uri = mock(Uri::class.java)
    private val otherFileUri: Uri = mock(Uri::class.java)

    private val attachmentFileStorage: AttachmentFileStorage = mock(AttachmentFileStorage::class.java)
    private val hopSigner: HopSigner = mock(HopSigner::class.java)
    private val hopNodeUrlProvider: HopNodeUrlProvider = mock(HopNodeUrlProvider::class.java)
    private val fileUploadRepository: FileUploadRepository = mock(FileUploadRepository::class.java)
    private val messageAttachmentUpdater: ChatMessageAttachmentUpdater = mock(ChatMessageAttachmentUpdater::class.java)
    private val attachmentMetaBuilder: AttachmentMetaBuilder = mock(AttachmentMetaBuilder::class.java)
    private val contentResolver: ContentResolver = mock(ContentResolver::class.java)
    private val fileProvider: FileProvider = mock(FileProvider::class.java)
    private val uploadConcurrency: UploadConcurrency = mock(UploadConcurrency::class.java)
    private val preProcessor: FileUploadPreProcessor = mock(FileUploadPreProcessor::class.java)
    private val hopService: HopService = mock(HopService::class.java)
    private val session: HopService.Session = mock(HopService.Session::class.java)
    private val submitTimes = mutableListOf<Duration>()
    private var onPreProcess: () -> Unit = {}

    private val nodes = RecordingHopNodes(hopSigner)

    private var inReadPhase = 0
    private var maxInReadPhase = 0

    @Before
    fun setUp() = runBlocking<Unit> {
        withSubmitAllowed(hopSigner)
        withImagePreProcessor()
    }

    @Test
    fun `an inline upload fails with a retryable error sixty seconds after its submit when the node never answers`() = runTest {
        nodes.withRealHopSigning()
        withFileBytes(attachmentFileStorage, fileUri, INLINE_FILE_SIZE)
        val uploader = uploaderOverRealHopService()

        val outcome = launchUpload(uploader, fileUpload(fileUri))
        runCurrent()
        val sentAt = nodes.sentRequests.single().sentAt

        advanceTo(sentAt + EXPECTED_HOP_REQUEST_BOUND - 1.milliseconds)
        assertActive(outcome)
        advanceTo(sentAt + EXPECTED_HOP_REQUEST_BOUND)
        assertFailedWithRetryable(outcome)
    }

    @Test
    fun `a chunk fails sixty seconds after it was sent when its submit never answers`() = runTest {
        withFileBytes(attachmentFileStorage, fileUri, 2 * HopService.CHUNK_SIZE_BYTES)
        withUploadStillActive(fileUploadRepository)
        withSessionAnsweringFirst(submits = 1, after = FIRST_CHUNK_RESPONSE_DELAY)

        val attemptStart = currentTime.milliseconds
        val outcome = launchUpload(uploaderOverMockedSession(), fileUpload(fileUri))
        advanceTo(attemptStart + FIRST_CHUNK_RESPONSE_DELAY)
        val secondChunkSentAt = submitTimes[1]

        advanceTo(attemptStart + EXPECTED_HOP_REQUEST_BOUND)
        assertActive(outcome)
        advanceTo(secondChunkSentAt + EXPECTED_HOP_REQUEST_BOUND - 1.milliseconds)
        assertActive(outcome)
        advanceTo(secondChunkSentAt + EXPECTED_HOP_REQUEST_BOUND)
        assertFailedWithRetryable(outcome)
    }

    @Test
    fun `a chunked root fails sixty seconds after it was sent when its submit never answers`() = runTest {
        withFileBytes(attachmentFileStorage, fileUri, HopService.CHUNK_SIZE_BYTES)
        withUploadStillActive(fileUploadRepository)
        withSessionAnsweringFirst(submits = 2)

        val outcome = launchUpload(uploaderOverMockedSession(), fileUpload(fileUri))
        runCurrent()
        val rootSentAt = submitTimes.last()

        advanceTo(rootSentAt + EXPECTED_HOP_REQUEST_BOUND - 1.milliseconds)
        assertActive(outcome)
        advanceTo(rootSentAt + EXPECTED_HOP_REQUEST_BOUND)
        assertFailedWithRetryable(outcome)
    }

    @Test
    fun `the session socket is stopped when a submit times out`() = runTest {
        nodes.withRealHopSigning()
        withFileBytes(attachmentFileStorage, fileUri, INLINE_FILE_SIZE)
        val uploader = uploaderOverRealHopService()

        launchUpload(uploader, fileUpload(fileUri))
        runCurrent()
        advanceTo(nodes.sentRequests.single().sentAt + EXPECTED_HOP_REQUEST_BOUND)

        verifySessionSocketStopped()
    }

    @Test
    fun `the pass returns at the bound when its submit is stuck`() = runTest {
        val pass = startPassWithStuckSubmit()

        advanceTo(nodes.sentRequests.single().sentAt + EXPECTED_HOP_REQUEST_BOUND)

        assertTrue("expected the pass to have returned", pass.isCompleted)
    }

    @Test
    fun `the upload is rescheduled with the first backoff and never failed when its submit is stuck`() = runTest {
        startPassWithStuckSubmit()

        advanceTo(nodes.sentRequests.single().sentAt + EXPECTED_HOP_REQUEST_BOUND)

        assertRescheduledWithFirstBackoff(MESSAGE_ID)
        verifyNeverFailed()
    }

    @Test
    fun `the next upload still reads when an earlier upload's whole read fails`() = runTest {
        launchUploadsAfterAFailingRead()

        runCurrent()

        verifyPreProcessedOnce()
        nodes.assertPendingSubmitsOnSeparateSessions(expected = 1)
    }

    @Test
    fun `an upload fails with FileNotFoundException when its whole read fails`() = runTest {
        val failed = launchUploadsAfterAFailingRead()

        runCurrent()

        assertFailureOf<FileNotFoundException>(failed.getCompleted())
    }

    @Test
    fun `image uploads pre-process one at a time when their submits overlap`() = runTest {
        nodes.withRealHopSigning()
        withCountedReads()
        val uploader = uploaderOverRealHopService(realAttachmentStorage())

        launchUpload(uploader, fileUpload(messageId = MESSAGE_ID, uri = fileUri))
        launchUpload(uploader, fileUpload(messageId = OTHER_MESSAGE_ID, uri = otherFileUri))
        runCurrent()

        assertEquals(1, maxInReadPhase)
        nodes.assertPendingSubmitsOnSeparateSessions(expected = 2)
    }

    @Test
    fun `an image is read and pre-processed before its session opens when it is uploaded`() = runTest {
        nodes.withRealHopSigning()
        withCountedReads()
        val uploader = uploaderOverRealHopService(realAttachmentStorage())

        launchUpload(uploader, fileUpload(fileUri, nodeUrl = NODE_A))
        runCurrent()

        assertHappensBefore(earlier = EVENT_OPEN, later = startEvent(NODE_A))
        assertHappensBefore(earlier = EVENT_PRE_PROCESS, later = startEvent(NODE_A))
    }

    private fun TestScope.uploaderOverRealHopService(
        storage: AttachmentFileStorage = attachmentFileStorage
    ): HopFileUploader = HopFileUploader(
        attachmentFileStorage = storage,
        hopService = nodes.hopService(this),
        hopSigner = hopSigner,
        fileUploadRepository = fileUploadRepository,
        messageAttachmentUpdater = messageAttachmentUpdater,
        attachmentMetaBuilder = attachmentMetaBuilder,
        preProcessors = setOf(preProcessor)
    )

    private fun uploaderOverMockedSession(): HopFileUploader = HopFileUploader(
        attachmentFileStorage = attachmentFileStorage,
        hopService = hopService,
        hopSigner = hopSigner,
        fileUploadRepository = fileUploadRepository,
        messageAttachmentUpdater = messageAttachmentUpdater,
        attachmentMetaBuilder = attachmentMetaBuilder,
        preProcessors = setOf(preProcessor)
    )

    private suspend fun TestScope.withSessionAnsweringFirst(submits: Int, after: Duration = Duration.ZERO) {
        whenever(hopService.withSession<Any?>(any(), any())).thenSuspend { invocation ->
            @Suppress("UNCHECKED_CAST")
            val block = invocation.arguments[1] as suspend HopService.Session.() -> Any?
            block(session)
        }
        whenever(session.submitEntry(any(), anyHopTicket())).thenSuspend {
            submitTimes += currentTime.milliseconds
            if (submitTimes.size > submits) awaitCancellation()
            delay(after)
            ByteArray(SUBMIT_HASH_SIZE) { submitTimes.size.toByte() }
        }
    }

    private suspend fun TestScope.startPassWithStuckSubmit(): Job {
        nodes.withRealHopSigning()
        withFileBytes(attachmentFileStorage, fileUri, INLINE_FILE_SIZE)
        withSinglePendingRow(fileUpload(fileUri))
        withUploadSlots(uploadConcurrency, MAX_IN_FLIGHT)
        val queue = FileUploadTransferQueue(fileUploadRepository, uploaderOverRealHopService(), hopNodeUrlProvider, uploadConcurrency)
        val processor = TransferQueueProcessor(HopTransferRetryPolicy(), FakeTimeProvider { testScheduler.currentTime })

        val pass = backgroundScope.launch { processor.processPendingConcurrently(queue) }
        runCurrent()
        return pass
    }

    private suspend fun TestScope.launchUploadsAfterAFailingRead(): Deferred<Result<Unit>> {
        nodes.withRealHopSigning()
        withCountedReads(missingUri = fileUri)
        val uploader = uploaderOverRealHopService(realAttachmentStorage())

        val failed = launchUpload(uploader, fileUpload(messageId = MESSAGE_ID, uri = fileUri))
        launchUpload(uploader, fileUpload(messageId = OTHER_MESSAGE_ID, uri = otherFileUri))
        return failed
    }

    private fun TestScope.realAttachmentStorage(): AttachmentFileStorage {
        return AttachmentFileStorage(contentResolver, fileProvider, testDispatchers())
    }

    private fun withImagePreProcessor() {
        withMarkingImagePreProcessor(preProcessor) { onPreProcess() }
    }

    private suspend fun withSinglePendingRow(upload: FileUpload) {
        whenever(fileUploadRepository.claimNextDue()).thenReturn(upload, null)
        whenever(fileUploadRepository.observeQueueChanges()).thenReturn(MutableSharedFlow())
        whenever(fileUploadRepository.getNextBackoffEndAt()).thenReturn(null)
    }

    private fun withCountedReads(missingUri: Uri? = null) {
        whenever(contentResolver.openInputStream(any())).thenAnswer { invocation ->
            if (invocation.arguments[0] == missingUri) {
                null
            } else {
                nodes.events += EVENT_OPEN
                inReadPhase++
                ByteArrayInputStream(patternBytes(INLINE_FILE_SIZE))
            }
        }
        onPreProcess = {
            nodes.events += EVENT_PRE_PROCESS
            maxInReadPhase = maxOf(maxInReadPhase, inReadPhase)
            inReadPhase--
        }
    }

    private suspend fun verifyNeverFailed() {
        verify(fileUploadRepository, never()).markFailed(any(), any())
    }

    private fun verifyPreProcessedOnce() {
        verify(preProcessor).preProcess(any())
    }

    private fun verifySessionSocketStopped() {
        verify(nodes.sockets.single()).stop()
    }

    private suspend fun assertRescheduledWithFirstBackoff(messageId: String) {
        val firstFailureAt: ArgumentCaptor<Instant> = ArgumentCaptor.forClass(Instant::class.java)
        val nextAttemptAt: ArgumentCaptor<Instant> = ArgumentCaptor.forClass(Instant::class.java)
        verify(fileUploadRepository).scheduleRetry(eq(messageId), eq(NODE_A), eq(1), capture(firstFailureAt), capture(nextAttemptAt))
        assertEquals(FIRST_BACKOFF, nextAttemptAt.value - firstFailureAt.value)
    }

    private fun assertActive(outcome: Deferred<Result<Unit>>) {
        assertFalse("expected the attempt to be still running", outcome.isCompleted)
    }

    private fun assertFailedWithRetryable(outcome: Deferred<Result<Unit>>) {
        assertTrue("expected the attempt to have completed", outcome.isCompleted)
        assertFailureOf<RetryableTransferException>(outcome.getCompleted())
    }

    private fun assertHappensBefore(earlier: String, later: String) {
        val earlierIndex = nodes.events.indexOf(earlier)
        val laterIndex = nodes.events.indexOf(later)
        assertTrue("expected $earlier before $later in ${nodes.events}", earlierIndex >= 0 && laterIndex > earlierIndex)
    }
}

private val FIRST_CHUNK_RESPONSE_DELAY = 50.seconds
private const val SUBMIT_HASH_SIZE = 32
private const val OTHER_MESSAGE_ID = "message-b"
private const val EVENT_OPEN = "open"
private const val EVENT_PRE_PROCESS = "preProcess"
