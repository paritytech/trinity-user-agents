package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.scan

sealed class ContactScanError(message: String, cause: Throwable?) : Throwable(message, cause) {
    class NotAnAddress(cause: Throwable) :
        ContactScanError("Scanned QR is not a decodable account address", cause)

    class SelfAddress :
        ContactScanError("Scanned address belongs to the current account", null)

    class ResolveFailed(cause: Throwable) :
        ContactScanError("Could not resolve chat data for the scanned address", cause)
}
