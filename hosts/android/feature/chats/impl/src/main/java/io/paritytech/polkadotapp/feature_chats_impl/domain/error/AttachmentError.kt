package io.paritytech.polkadotapp.feature_chats_impl.domain.error

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.FileTooLargeException

sealed class AttachmentError(message: String) : Throwable(message) {
    class FileTooLarge(val maxSize: InformationSize) : AttachmentError("attachment exceeds $maxSize")

    class Unknown(override val cause: Throwable) : AttachmentError("failed to attach the picked file")
}

fun Throwable.asAttachmentError(): AttachmentError = when (this) {
    is AttachmentError -> this
    is FileTooLargeException -> AttachmentError.FileTooLarge(maxSize)

    else -> AttachmentError.Unknown(this)
}
