package io.paritytech.polkadotapp.feature_chats_impl.domain.hop

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferDirection
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.download.FileDownloadStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileDownloadRepository
import javax.inject.Inject

class MultimediaMessageControlInteractor @Inject constructor(
    private val downloadRepository: FileDownloadRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val downloadStarter: FileDownloadStarter
) {
    suspend fun redownload(messageId: ChatMessageId) {
        downloadRepository.redownload(messageId)
        downloadStarter.startDownloading()
    }

    suspend fun cancel(messageId: ChatMessageId, direction: FileTransferDirection) {
        when (direction) {
            FileTransferDirection.UPLOAD -> chatMessageRepository.removeMessage(messageId)
            FileTransferDirection.DOWNLOAD -> downloadRepository.cancel(messageId)
        }
    }
}
