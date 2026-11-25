package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId

sealed interface StatementStoreSlotHolder {
    val accountId: AccountId

    data class Local(override val accountId: AccountId, val derivationPath: String) : StatementStoreSlotHolder

    data class Remote(override val accountId: AccountId) : StatementStoreSlotHolder
}
