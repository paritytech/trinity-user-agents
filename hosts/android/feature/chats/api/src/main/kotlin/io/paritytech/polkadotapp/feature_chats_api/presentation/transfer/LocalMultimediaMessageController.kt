package io.paritytech.polkadotapp.feature_chats_api.presentation.transfer

import android.net.Uri
import androidx.compose.runtime.staticCompositionLocalOf
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf

val LocalMultimediaMessageController = staticCompositionLocalOf<MultimediaMessageController> {
    NoOpMultimediaMessageController
}

private object NoOpMultimediaMessageController : MultimediaMessageController {
    override fun observe(id: ChatMessageId): Flow<FileTransferUiState?> = flowOf(null)
    override fun redownload(messageId: ChatMessageId) = Unit
    override fun cancel(messageId: ChatMessageId) = Unit
    override fun openImage(uri: Uri) = Unit
}
