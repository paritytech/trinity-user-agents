package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

sealed class SignVrfError(message: String) : Throwable(message) {
    data object Rejected : SignVrfError("VRF signing was rejected")
    class TranscriptTooLarge(reason: String) : SignVrfError(reason)
    class Unknown(reason: String) : SignVrfError(reason)
}
