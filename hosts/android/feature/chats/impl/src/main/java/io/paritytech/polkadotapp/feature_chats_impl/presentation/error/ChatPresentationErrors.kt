package io.paritytech.polkadotapp.feature_chats_impl.presentation.error

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.AttachmentError
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.ChatRequestError
import io.paritytech.polkadotapp.common.R as RCommon

fun AttachmentError.toPresentationError(): PresentationThrowable = when (this) {
    is AttachmentError.FileTooLarge -> FileTooLargePresentationError(maxSize, this)

    is AttachmentError.Unknown -> UnexpectedPresentationError(this)
}

fun ChatRequestError.toPresentationError(): PresentationThrowable = when (this) {
    ChatRequestError.ContactNotFound -> ContactNotFoundPresentationError(this)

    is ChatRequestError.Unknown -> UnexpectedPresentationError(this)
}

class ContactNotFoundPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.chat_error_contact_not_found)

// Carries the limit the size check actually enforced, so the message can never drift from it.
class FileTooLargePresentationError(
    private val maxSize: InformationSize,
    cause: Throwable,
) : PresentationThrowable(cause) {
    @Composable
    override fun message(): String {
        return stringResource(RCommon.string.chat_error_attachment_too_large, maxSize.inWholeMegabytes)
    }
}

// The call could not start rather than failed, so there is no cause to carry.
class BusyInAnotherChatPresentationError :
    PresentationThrowable(cause = null),
    PresentationError by StringResPresentationError(RCommon.string.chat_call_busy_in_another_chat)
