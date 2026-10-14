package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

sealed class RegisterRingVrfKeyError(message: String) : Throwable(message) {
    data object NotConnected : RegisterRingVrfKeyError("No user is signed in")
    data object RingNotFound : RegisterRingVrfKeyError("Ring not found for the requested location")
    data object Rejected : RegisterRingVrfKeyError("Key registration was rejected")
    class Unknown(reason: String) : RegisterRingVrfKeyError(reason)
}

sealed class ListRingVrfKeysError(message: String) : Throwable(message) {
    data object NotConnected : ListRingVrfKeysError("No user is signed in")
    data object Rejected : ListRingVrfKeysError("Listing another product's keys was rejected")
    class Unknown(reason: String) : ListRingVrfKeysError(reason)
}

sealed class RingVrfSignError(message: String) : Throwable(message) {
    data object NotConnected : RingVrfSignError("No user is signed in")
    data object KeyNotRegistered : RingVrfSignError("Key handle has no registry entry")
    data object NotAllowlisted : RingVrfSignError("Key handle is foreign and its owner has not allowlisted the caller")
    data object Rejected : RingVrfSignError("Ring VRF signing was rejected")
    class Unknown(reason: String) : RingVrfSignError(reason)
}
