package io.paritytech.polkadotapp.feature_account_impl.data.mappers

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.database.model.MetaAccountLocal
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

fun MetaAccountLocal.toDomain(): MetaAccount {
    return MetaAccount(
        id = id,
        substrateCryptoType = substrateCryptoType.toDomain(),
        substrateAccountId = substrateAccountId.intoAccountId(),
        name = name,
        signerType = signerType.toDomain(),
        purpose = purpose.toDomain()
    )
}

private fun MetaAccountLocal.SignerTypeLocal.toDomain(): MetaAccount.SignerType {
    return when (this) {
        MetaAccountLocal.SignerTypeLocal.SECRETS -> MetaAccount.SignerType.SECRETS
    }
}

fun MetaAccountLocal.PurposeLocal.toDomain(): MetaAccount.Purpose {
    return when (this) {
        MetaAccountLocal.PurposeLocal.WALLET -> MetaAccount.Purpose.WALLET
        MetaAccountLocal.PurposeLocal.DEPOSIT -> MetaAccount.Purpose.DEPOSIT
        MetaAccountLocal.PurposeLocal.ALIAS -> MetaAccount.Purpose.ALIAS
        MetaAccountLocal.PurposeLocal.CANDIDATE -> MetaAccount.Purpose.CANDIDATE
    }
}
