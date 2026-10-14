package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionIdWithIndex

data class RecyclerKey(
    val exponent: ValueExponent,
    val recyclerIndex: RecyclerIndex
)

fun RecyclerKey.toStorageKey(instanceId: CoinageInstanceId): RingCollectionIdWithIndex =
    exponent.toRingCollectionId(instanceId) to recyclerIndex

fun RingCollectionIdWithIndex.toRecyclerKey(): RecyclerKey {
    val exponentByte = first.value.value[20].toInt()
    return RecyclerKey(
        exponent = ValueExponent(exponentByte),
        recyclerIndex = second
    )
}
