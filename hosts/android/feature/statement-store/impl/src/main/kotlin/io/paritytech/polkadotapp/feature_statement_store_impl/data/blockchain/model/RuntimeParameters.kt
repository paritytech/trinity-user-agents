package io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import kotlinx.serialization.Serializable

@Serializable
sealed class RuntimeParametersKey {
    @Serializable
    @TransientStruct
    class StatementStorage(val key: StatementStorageParametersKey) : RuntimeParametersKey()
}

@Serializable
sealed class StatementStorageParametersKey {
    @Serializable
    class NotificationAllowance : StatementStorageParametersKey()
}

@Serializable
sealed class RuntimeParametersValue {
    @Serializable
    @TransientStruct
    class StatementStorage(val value: StatementStorageParametersValue) : RuntimeParametersValue()
}

@Serializable
sealed class StatementStorageParametersValue {
    @Serializable
    @TransientStruct
    class NotificationAllowance(val allowance: StatementAllowance) : StatementStorageParametersValue()
}
