package io.paritytech.polkadotapp.common.domain.model

enum class AddressScheme {
    /**
     * 32-byte address, ss58 address encoding
     */
    SUBSTRATE,

    /**
     * 20-byte address, Ethereum-like address encoding
     */
    EVM;

    companion object {
        fun findFromAccountId(accountId: AccountId): AddressScheme? {
            return when (accountId.value.size) {
                32 -> SUBSTRATE
                20 -> EVM
                else -> null
            }
        }
    }
}

fun AccountId.getAddressScheme(): AddressScheme? {
    return AddressScheme.findFromAccountId(this)
}

fun AccountId.getAddressSchemeOrThrow(): AddressScheme {
    return requireNotNull(getAddressScheme()) {
        "Could not detect address scheme from account id of length ${value.size}"
    }
}

fun AddressScheme.isSubstrate(): Boolean {
    return this == AddressScheme.SUBSTRATE
}
