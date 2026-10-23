package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import android.content.Context
import android.net.Uri
import android.webkit.MimeTypeMap
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.common.utils.openImage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferDirection
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferUiState
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.MultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject

/**
 * The media of the product messages of one room screen, drawn as the native chat draws its attachments: the host loads
 * each file when the message shows, as the native chat downloads an attachment, and the transfer overlay shows the
 * download, and the upload of an attachment that waits in the outbox.
 */
class ProductRoomMedia(
    private val scope: CoroutineScope,
    private val context: Context,
    private val loader: ChatMediaLoader,
    private val fileProvider: FileProvider,
    private val attachmentStager: ProductAttachmentStager,
) : MultimediaMessageController {
    private sealed interface Load {
        /** [progress] follows the bytes that the core reports, as the native download does. */
        data class Running(val progress: Fraction) : Load

        data class Done(val uri: Uri) : Load

        data object Failed : Load
    }

    /** [outboxStatus] is the status of an attachment that waits in the outbox, and null for any other. */
    private data class MediaRef(val resource: ChatResourceModel, val mimeType: String, val outboxStatus: ChatMessage.Status?)

    private val loads = MutableStateFlow<Map<ChatResourceModel, Load>>(emptyMap())
    private val refs = MutableStateFlow<Map<ChatMessageId, MediaRef>>(emptyMap())
    private val jobs = ConcurrentHashMap<ChatResourceModel, Job>()

    /** The local files of the media that loaded. */
    val files: Flow<Map<ChatResourceModel, Uri>> = loads
        .map { all -> all.mapNotNull { (resource, load) -> (load as? Load.Done)?.let { resource to it.uri } }.toMap() }
        .distinctUntilChanged()

    /** Starts to load the media of [messages] that did not load yet. */
    fun track(messages: List<ChatMessage>) {
        val tracked = messages.mapNotNull { message -> message.mediaRef()?.let { message.id to it } }.toMap()
        refs.value = tracked
        tracked.values.forEach { ref -> if (ref.resource !in loads.value) start(ref) }
    }

    override fun observe(id: ChatMessageId): Flow<FileTransferUiState?> =
        combine(refs, loads, attachmentStager.observeUploads()) { refs, loads, uploads ->
            refs[id]?.let { ref -> ref.transferState(loads[ref.resource], uploads[id]) }
        }.distinctUntilChanged()

    override fun redownload(messageId: ChatMessageId) {
        val ref = refs.value[messageId] ?: return
        if (loads.value[ref.resource] == Load.Failed) start(ref)
    }

    // An attachment that waits in the outbox goes away with its upload. A download stops, as a native one, and the
    // bubble offers it again.
    override fun cancel(messageId: ChatMessageId) {
        val ref = refs.value[messageId] ?: return
        if (ref.outboxStatus != null) {
            scope.launch { attachmentStager.delete(messageId) }
            return
        }
        jobs.remove(ref.resource)?.cancel()
        loads.update { all -> if (all[ref.resource] is Load.Running) all + (ref.resource to Load.Failed) else all }
    }

    override fun openImage(uri: Uri) {
        context.openImage(uri)
    }

    private fun start(ref: MediaRef) {
        loads.update { it + (ref.resource to Load.Running(Fraction.ZERO)) }
        jobs[ref.resource] = scope.launch {
            val extension = MimeTypeMap.getSingleton().getExtensionFromMimeType(ref.mimeType)
            val progress = { received: Long, total: Long ->
                // The core reports in order; a report after the end of the load changes nothing.
                loads.update { all ->
                    if (all[ref.resource] !is Load.Running) return@update all
                    all + (ref.resource to Load.Running(progressOf(received, total)))
                }
            }
            val load = loader.load(ref.resource, extension, progress).fold(
                onSuccess = { Load.Done(fileProvider.uriOf(it)) },
                onFailure = { error ->
                    Timber.w(error, "The media of a product message did not load")
                    Load.Failed
                },
            )
            loads.update { it + (ref.resource to load) }
            jobs.remove(ref.resource)
        }
    }

    private fun MediaRef.transferState(load: Load?, upload: ProductAttachmentStager.UploadProgress?): FileTransferUiState? = when {
        outboxStatus == ChatMessage.Status.DELIVERY_FAILED -> FileTransferUiState.Failed(FileTransferDirection.UPLOAD)
        outboxStatus == ChatMessage.Status.PROCESSING -> FileTransferUiState.InProgress(
            FileTransferDirection.UPLOAD,
            upload?.let { progressOf(it.sent, it.total) } ?: Fraction.ZERO,
        )
        // The upload ended; the message waits for the product as a native message waits to be sent.
        outboxStatus != null -> null
        load is Load.Running -> FileTransferUiState.InProgress(FileTransferDirection.DOWNLOAD, load.progress)
        load == Load.Failed -> FileTransferUiState.Cancelled(FileTransferDirection.DOWNLOAD)
        else -> null
    }

    private fun ChatMessage.mediaRef(): MediaRef? {
        val body = (content as? ChatMessage.Content.ProductBody)?.body as? ProductMessageBody.RichText ?: return null
        val item = body.media.firstOrNull() ?: return null
        val outboxStatus = status.takeIf { attachmentStager.isOutboxPreview(item.resource) }
        return MediaRef(item.resource, item.meta.mimeType, outboxStatus)
    }

    // As the native transfer state counts its chunks: whole percents, from 0 to 100.
    private fun progressOf(received: Long, total: Long): Fraction {
        if (total <= 0) return 0.percents
        return (received * PERCENT / total).coerceIn(0, PERCENT).toInt().percents
    }

    class Factory @Inject constructor(
        @param:ApplicationContext private val context: Context,
        private val loader: ChatMediaLoader,
        private val fileProvider: FileProvider,
        private val attachmentStager: ProductAttachmentStager,
    ) {
        fun create(scope: CoroutineScope) = ProductRoomMedia(scope, context, loader, fileProvider, attachmentStager)
    }

    private companion object {
        const val PERCENT = 100L
    }
}
