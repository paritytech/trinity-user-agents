package io.paritytech.polkadotapp.feature_sso_impl.domain.session.model

import io.paritytech.polkadotapp.common.utils.InformationSize

sealed class SendMessageError(override val cause: Throwable?) : Throwable() {
    class MessageTooLarge(
        val requestedSize: InformationSize,
        val maxAllowedSize: InformationSize
    ) : SendMessageError(cause = null) {
        override val message: String
            get() = "Too large message: $requestedSize, max: $maxAllowedSize"
    }

    class SubmissionFailed(cause: Throwable) : SendMessageError(cause)
}
