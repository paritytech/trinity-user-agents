package io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.serialization.Serializable

@Serializable
sealed class TransactionStorageAuthorizationScope {
    @Serializable
    @TransientStruct
    class Account(val accountId: AccountId) : TransactionStorageAuthorizationScope()
}
