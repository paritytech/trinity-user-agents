package io.paritytech.polkadotapp.feature_chats_api.presentation.transfer

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.utils.Fraction

@Immutable
sealed interface FileTransferUiState {
    val direction: FileTransferDirection

    data class InProgress(
        override val direction: FileTransferDirection,
        val progress: Fraction
    ) : FileTransferUiState

    data class Failed(
        override val direction: FileTransferDirection
    ) : FileTransferUiState

    data class Cancelled(
        override val direction: FileTransferDirection
    ) : FileTransferUiState
}

enum class FileTransferDirection {
    UPLOAD, DOWNLOAD
}
