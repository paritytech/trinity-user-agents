package io.paritytech.polkadotapp.feature_chats_impl.presentation.error

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentFailure
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

/** What the room says when the host did not send an attachment that it stages (07-LATER §7). */
fun ProductAttachmentFailure.toPresentationError(): PresentationThrowable {
    val cause = ProductAttachmentNotSentException(this)
    return when (this) {
        // The chain bound comes from the core, so the message names the bound that refused the file.
        is ProductAttachmentFailure.TooLarge -> FileTooLargePresentationError(maxBytes.bytes, cause)
        ProductAttachmentFailure.PermissionDenied -> AttachmentPermissionPresentationError(cause)
        ProductAttachmentFailure.NotDeclared -> AttachmentNotSentPresentationError("the room does not take files over 1 MB", cause)
        is ProductAttachmentFailure.AllowanceUnavailable -> AttachmentNotSentPresentationError("no Bulletin allowance ($reason)", cause)
        is ProductAttachmentFailure.Failed -> AttachmentNotSentPresentationError(reason, cause)
        is ProductAttachmentFailure.Refused -> AttachmentNotSentPresentationError(reason ?: "the app refused it", cause)
    }
}

class ProductAttachmentNotSentException(val failure: ProductAttachmentFailure) : Exception("attachment not sent: $failure")

class AttachmentPermissionPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.chat_attachment_not_sent_permission)

class AttachmentNotSentPresentationError(
    private val reason: String,
    cause: Throwable,
) : PresentationThrowable(cause) {
    @Composable
    override fun message(): String = stringResource(RCommon.string.chat_attachment_not_sent, reason)
}

// The call could not start rather than failed, so there is no cause to carry.
class BusyInAnotherChatPresentationError :
    PresentationThrowable(cause = null),
    PresentationError by StringResPresentationError(RCommon.string.chat_call_busy_in_another_chat)
