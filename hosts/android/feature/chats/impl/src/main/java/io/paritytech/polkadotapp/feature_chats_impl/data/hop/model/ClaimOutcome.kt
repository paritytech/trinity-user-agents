package io.paritytech.polkadotapp.feature_chats_impl.data.hop.model

sealed interface ClaimOutcome {
    class Found(val data: ByteArray) : ClaimOutcome
    object NotFound : ClaimOutcome
    class Retryable(val code: Int, val message: String) : ClaimOutcome
}
