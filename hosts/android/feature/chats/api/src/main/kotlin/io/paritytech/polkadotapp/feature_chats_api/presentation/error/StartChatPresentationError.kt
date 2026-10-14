package io.paritytech.polkadotapp.feature_chats_api.presentation.error

import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.feature_chats_api.domain.error.StartChatError
import io.paritytech.polkadotapp.common.R as RCommon

class PeerNotAvailablePresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.chat_error_peer_not_available)

// Lives in api rather than impl since chat starting is also driven from feature/wallet
fun StartChatError.toPresentationError(): PresentationThrowable = when (this) {
    StartChatError.PeerNotRegistered -> PeerNotAvailablePresentationError(this)

    is StartChatError.Unknown -> UnexpectedPresentationError(this)
}
