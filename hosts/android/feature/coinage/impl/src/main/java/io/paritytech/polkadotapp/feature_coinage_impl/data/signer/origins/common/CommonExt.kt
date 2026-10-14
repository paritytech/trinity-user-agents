package io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.common

import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerLocationOrThrow

fun List<RecyclerVoucher>.getCommonRecyclerKey(): RecyclerKey {
    val recyclerKeys = mapToSet {
        RecyclerKey(
            exponent = it.recyclerValue,
            recyclerIndex = it.recyclerLocationOrThrow().recyclerIndex
        )
    }
    require(recyclerKeys.size == 1) // Validate all vouchers in the same recycler

    return recyclerKeys.first()
}
