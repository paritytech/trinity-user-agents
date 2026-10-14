package io.paritytech.polkadotapp.feature_w3spay_impl.domain

/**
 * Distinct, loggable reasons a W3S / DSFinV-K scan was rejected. The wallet scanner collapses every
 * failure into a generic "invalid code" message, so these exist to make the actual cause visible in
 * logs (and to allow a specific user-facing message later).
 */
sealed class W3sScanError(message: String, cause: Throwable?) : Throwable(message, cause) {
    /** The QR claimed to be a Kassenbeleg-V1 receipt but could not be parsed. */
    class UnreadableReceipt(cause: Throwable) :
        W3sScanError("Could not read DSFinV-K receipt", cause)

    /** The `w3s-merchants` remote config could not be fetched or parsed. */
    class MerchantConfigUnavailable(serialNumber: String, cause: Throwable) :
        W3sScanError("W3S merchant config unavailable for serial '$serialNumber'", cause)

    /** The config is readable, but no merchant is registered for this cash-register serial. */
    class UnknownMerchant(serialNumber: String) :
        W3sScanError("No W3S merchant registered for serial '$serialNumber'", null)
}
