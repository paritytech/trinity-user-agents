package io.paritytech.polkadotapp.bandersnatch_crypto

/**
 * Domain sizes for the PCS (Polynomial Commitment Scheme).
 * Determines the maximum ring size that can be supported.
 *
 * Ordinal values are used to communicate with the native layer.
 */
enum class BandersnatchDomainSize(val maxMembers: Int) {
    /** Domain size 2^11, max 255 members */
    Domain11(maxMembers = 255),

    /** Domain size 2^12, max 767 members */
    Domain12(maxMembers = 767),

    /** Domain size 2^16, max 16127 members */
    Domain16(maxMembers = 16127)
}
