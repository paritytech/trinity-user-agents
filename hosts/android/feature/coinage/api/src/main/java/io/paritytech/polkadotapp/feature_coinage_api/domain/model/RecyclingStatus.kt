package io.paritytech.polkadotapp.feature_coinage_api.domain.model

sealed interface RecyclingStatus {
    data object Pending : RecyclingStatus

    /** Every coin became a voucher, and each of [vouchers] is already seen in its recycler. */
    data class AllRecycled(
        val vouchers: List<RecyclerVoucher>,
        val finalized: Boolean,
    ) : RecyclingStatus

    /** At least one coin will never become a voucher. */
    data object Incomplete : RecyclingStatus
}
