package io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.serialization.Serializable

@Serializable
data class StmtStoreAllowanceEntry(
    val accountId: AccountId,
    val seq: UInt,
    val since: ULong,
)
