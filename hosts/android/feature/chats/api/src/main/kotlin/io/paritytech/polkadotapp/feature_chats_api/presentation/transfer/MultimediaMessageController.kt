package io.paritytech.polkadotapp.feature_chats_api.presentation.transfer

import android.net.Uri
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.dynamic.PerMessageStateProvider

interface MultimediaMessageController : PerMessageStateProvider<FileTransferUiState> {
    fun redownload(messageId: ChatMessageId)
    fun cancel(messageId: ChatMessageId)
    fun openImage(uri: Uri)
}
