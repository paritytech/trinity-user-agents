package io.paritytech.polkadotapp.feature_members_api.data.model

import kotlinx.serialization.Serializable

@Serializable
class RingMembersState(
    val mode: RingMutationMode,
) {
    internal val isAppendOnly: Boolean
        get() = mode is RingMutationMode.AppendOnly
}

@Serializable
sealed interface RingMutationMode {
    @Serializable
    object AppendOnly : RingMutationMode

    @Serializable
    class Mutating(val value: UByte) : RingMutationMode
}
