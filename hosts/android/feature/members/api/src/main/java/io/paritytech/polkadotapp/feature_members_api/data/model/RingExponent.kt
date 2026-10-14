package io.paritytech.polkadotapp.feature_members_api.data.model

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchDomainSize
import kotlinx.serialization.Serializable

@Serializable
enum class RingExponent(val exponent: Int) {
    R2e9(9),
    R2e10(10),
    R2e14(14);

    /**
     * How many keys the ring can hold. Smaller than `2^exponent`: ring-VRF reserves the tail of the domain
     * for PIOP padding, `4 + MODULUS_BIT_SIZE` of the Bandersnatch scalar field. So [R2e10], which recycler
     * collections use, holds 767 members rather than 1024.
     */
    val ringCapacity: Int
        get() = (1 shl exponent) - RING_PIOP_OVERHEAD
}

/**
 * Top-level rather than in a companion: reading a private companion const from an enum member's getter
 * crashes the Kotlin backend in SyntheticAccessorLowering.
 */
private const val RING_PIOP_OVERHEAD = 257

fun RingExponent.toDomainSize(): BandersnatchDomainSize {
    return when (this) {
        RingExponent.R2e9 -> BandersnatchDomainSize.Domain11
        RingExponent.R2e10 -> BandersnatchDomainSize.Domain12
        RingExponent.R2e14 -> BandersnatchDomainSize.Domain16
    }
}
