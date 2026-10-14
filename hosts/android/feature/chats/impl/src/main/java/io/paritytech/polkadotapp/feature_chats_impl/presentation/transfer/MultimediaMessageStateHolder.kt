package io.paritytech.polkadotapp.feature_chats_impl.presentation.transfer

import android.content.Context
import android.net.Uri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.openImage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferUiState
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.MultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileDownloadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.MultimediaMessageControlInteractor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.dynamic.PerMessageStateHolder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class MultimediaMessageStateHolder @Inject constructor(
    uploadRepository: FileUploadRepository,
    downloadRepository: FileDownloadRepository,
    mapper: FileTransferUiStateMapper,
    private val interactor: MultimediaMessageControlInteractor,
    @param:ApplicationContext private val appContext: Context
) : PerMessageStateHolder<FileTransferUiState>(
    scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
    source = combine(
        uploadRepository.observeActive(),
        downloadRepository.observeActive()
    ) { uploads, downloads ->
        mapper.merge(uploads, downloads)
    }
), MultimediaMessageController {
    override fun redownload(messageId: ChatMessageId) {
        currentState(messageId) as? FileTransferUiState.Cancelled ?: return
        scope.launch {
            interactor.redownload(messageId)
        }
    }

    override fun cancel(messageId: ChatMessageId) {
        val inProgress = currentState(messageId) as? FileTransferUiState.InProgress ?: return
        scope.launch {
            interactor.cancel(messageId, inProgress.direction)
        }
    }

    override fun openImage(uri: Uri) {
        appContext.openImage(uri)
    }
}
