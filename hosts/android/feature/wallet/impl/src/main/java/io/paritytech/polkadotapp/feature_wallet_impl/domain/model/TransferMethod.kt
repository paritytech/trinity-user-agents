package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

sealed interface TransferMethod {
    data class CoinsViaChat(val recipient: AccountId) : TransferMethod

    data class UnloadIntoExternal(val recipient: AccountId) : TransferMethod

    data class CoinsViaSubmitter(
        val submitterId: String,
        val submitterPayload: ByteArray,
    ) : TransferMethod
}
