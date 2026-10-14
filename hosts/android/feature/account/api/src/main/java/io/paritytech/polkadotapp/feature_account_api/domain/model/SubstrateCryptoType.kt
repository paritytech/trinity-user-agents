package io.paritytech.polkadotapp.feature_account_api.domain.model

import io.novasama.substrate_sdk_android.encrypt.EncryptionType

enum class SubstrateCryptoType {
    SR25519,
    ED25519,
    ECDSA,
}

fun SubstrateCryptoType.toEncryption(): EncryptionType {
    return when (this) {
        SubstrateCryptoType.SR25519 -> EncryptionType.SR25519
        SubstrateCryptoType.ED25519 -> EncryptionType.ED25519
        SubstrateCryptoType.ECDSA -> EncryptionType.ECDSA
    }
}

fun EncryptionType.toSubstrateCryptoType(): SubstrateCryptoType {
    return when (this) {
        EncryptionType.SR25519 -> SubstrateCryptoType.SR25519
        EncryptionType.ED25519 -> SubstrateCryptoType.ED25519
        EncryptionType.ECDSA -> SubstrateCryptoType.ECDSA
    }
}
