package io.paritytech.polkadotapp.feature_chats_impl.data.hop.download

import android.content.Context
import androidx.work.WorkerParameters
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueueProcessor
import kotlinx.coroutines.runBlocking
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

/** Every launch enqueues the worker, so its ongoing notification must only show when a download is due. */
class FileDownloadWorkerTest {
    private val context: Context = mockk(relaxed = true)
    private val transferQueue: FileDownloadTransferQueue = mockk {
        coEvery { soonestNextAttemptAt() } returns null
    }

    private val worker = FileDownloadWorker(
        appContext = context,
        parameters = mockk<WorkerParameters>(relaxed = true),
        transferQueueProcessor = mockk<TransferQueueProcessor>(relaxed = true),
        transferQueue = transferQueue,
    )

    @Test
    fun `an empty queue finishes without a notification`() = runBlocking<Unit> {
        coEvery { transferQueue.nextPending() } returns null

        worker.doWork()

        verify(exactly = 0) { context.getString(RCommon.string.workers_notification_channel_id) }
    }

    @Test
    fun `a pending download runs in the foreground`() = runBlocking<Unit> {
        coEvery { transferQueue.nextPending() } returns mockk()

        worker.doWork()

        verify(exactly = 1) { context.getString(RCommon.string.workers_notification_channel_id) }
    }
}
