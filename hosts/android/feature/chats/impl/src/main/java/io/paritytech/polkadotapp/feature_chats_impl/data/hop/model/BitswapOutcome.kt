package io.paritytech.polkadotapp.feature_chats_impl.data.hop.model

sealed interface BitswapOutcome {
    class Found(val data: ByteArray) : BitswapOutcome
    object NotFound : BitswapOutcome
    object InvalidCid : BitswapOutcome
    class Retryable(val code: Int, val message: String) : BitswapOutcome
}
