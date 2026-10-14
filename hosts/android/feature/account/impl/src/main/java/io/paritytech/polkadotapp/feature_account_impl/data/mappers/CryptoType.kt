package io.paritytech.polkadotapp.feature_account_impl.data.mappers

import io.paritytech.polkadotapp.database.model.MetaAccountLocal.SubstrateCryptoTypeLocal
import io.paritytech.polkadotapp.feature_account_api.domain.model.SubstrateCryptoType

fun SubstrateCryptoType.toLocal(): SubstrateCryptoTypeLocal {
    return when (this) {
        SubstrateCryptoType.SR25519 -> SubstrateCryptoTypeLocal.SR25519
        SubstrateCryptoType.ED25519 -> SubstrateCryptoTypeLocal.ED25519
        SubstrateCryptoType.ECDSA -> SubstrateCryptoTypeLocal.ECDSA
    }
}

fun SubstrateCryptoTypeLocal.toDomain(): SubstrateCryptoType {
    return when (this) {
        SubstrateCryptoTypeLocal.SR25519 -> SubstrateCryptoType.SR25519
        SubstrateCryptoTypeLocal.ED25519 -> SubstrateCryptoType.ED25519
        SubstrateCryptoTypeLocal.ECDSA -> SubstrateCryptoType.ECDSA
    }
}
