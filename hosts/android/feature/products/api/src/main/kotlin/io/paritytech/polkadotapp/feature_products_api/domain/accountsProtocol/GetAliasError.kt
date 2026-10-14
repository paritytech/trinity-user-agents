package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

sealed class GetAliasError(message: String) : Throwable(message) {
    data object RingNotFound : GetAliasError("Ring not found for the requested location")
    data object NotMember : GetAliasError("Selected member key is not a member of the requested ring")
    data object KeyNotRegistered : GetAliasError("Key handle has no registry entry")
    data object KeyNotInRing : GetAliasError("Key handle is registered, but not for the requested ring")
    data object Rejected : GetAliasError("Alias derivation was rejected")
    class Unknown(reason: String) : GetAliasError(reason)
}
