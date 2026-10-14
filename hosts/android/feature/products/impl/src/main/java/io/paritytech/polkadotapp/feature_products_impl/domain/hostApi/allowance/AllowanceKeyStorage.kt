package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance

import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

enum class AllowanceResourceKind {
    BULLETIN,
    STATEMENT_STORE,
}

interface AllowanceKeyStorage {
    suspend fun get(productId: ProductId, kind: AllowanceResourceKind): SlotAccountKey?

    suspend fun put(productId: ProductId, kind: AllowanceResourceKind, key: SlotAccountKey)
}
