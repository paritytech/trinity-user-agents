package io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chain_resources_api.data.model.NotificationReference
import kotlinx.serialization.Serializable

@Serializable
class NotificationRegistrationEntry(
    val accountId: AccountId,
    val reference: NotificationReference,
)
