package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

sealed class CreateProofError(message: String) : Throwable(message) {
    data object RingNotFound : CreateProofError("Ring not found for the requested location")
    data object NotMember : CreateProofError("Selected member key is not a member of the requested ring")
    data object KeyNotRegistered : CreateProofError("Key handle has no registry entry")
    data object KeyNotInRing : CreateProofError("Key handle is registered, but not for the requested ring")
    data object NotAllowlisted : CreateProofError("Key handle is foreign and its owner has not allowlisted the caller")
    data object Rejected : CreateProofError("Proof creation was rejected")
    class Unknown(reason: String) : CreateProofError(reason)
}
