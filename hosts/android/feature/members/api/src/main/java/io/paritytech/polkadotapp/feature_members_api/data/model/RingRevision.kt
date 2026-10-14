package io.paritytech.polkadotapp.feature_members_api.data.model

import kotlinx.serialization.Serializable

@JvmInline
@Serializable
value class RingRevision(val value: Int) : Comparable<RingRevision> {
    override fun compareTo(other: RingRevision): Int {
        return value.compareTo(other.value)
    }
}
