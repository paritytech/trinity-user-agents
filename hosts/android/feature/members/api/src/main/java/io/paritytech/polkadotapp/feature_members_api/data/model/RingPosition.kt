package io.paritytech.polkadotapp.feature_members_api.data.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlinx.serialization.Serializable

@Serializable
sealed class RingPosition {
    @Serializable
    class Onboarding(
        val queuePage: Int,
        val queuedAt: Timestamp
    ) : RingPosition()

    @Serializable
    class Included(
        val ringIndex: RingIndex,
        val ringPage: PageIndex,
        val ringPosition: Int
    ) : RingPosition()

    @Serializable
    object Suspended : RingPosition()
}

val RingPosition.ringIndex: RingIndex?
    get() = (this as? RingPosition.Included)?.ringIndex

fun RingPosition.requireIncludedPosition(): RingPosition.Included = includedOrFailure().getOrThrow()

fun RingPosition.includedOrFailure(): Result<RingPosition.Included> = when (this) {
    is RingPosition.Included -> Result.success(this)
    else -> Result.failure(IllegalArgumentException("Ring member doesn't included into the ring"))
}

fun RingPosition.includedOrNull(): RingPosition.Included? = includedOrFailure().getOrNull()

val RingPosition.ringPosition: Int?
    get() = (this as? RingPosition.Included)?.ringPosition
