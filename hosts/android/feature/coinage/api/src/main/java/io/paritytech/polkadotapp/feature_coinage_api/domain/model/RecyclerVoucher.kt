package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import kotlin.time.Instant

typealias RecyclerIndex = RingIndex

data class RecyclerVoucher(
    val ringVrfKeyIndex: CoinageKeyIndex,
    val ringVrfPublicKey: BandersnatchPublicKey,
    val recyclerValue: ValueExponent,
    val location: Location,
    /** Kept current by `VoucherLocationService` for as long as the voucher sits in a ring. */
    val recyclerFungibility: RecyclerFungibility,
    /**
     * Frozen on the voucher's first transition into a recycler, or null before that has happened — the
     * unloaded count it is measured against cannot be read until the ring index exists.
     *
     * Null rather than [RecyclerFungibility.NONE] for "not frozen yet", because a ring that was already
     * drained when the voucher landed in it has a genuine maximum of zero, and that value must never be
     * overwritten afterwards.
     *
     * The runtime can later *decrement* the unloaded count when an alias is marked unloaded, so
     * [recyclerFungibility] may legitimately overtake this. Consumers clamp rather than treat it as a bug.
     */
    val maxRecyclerFungibility: RecyclerFungibility?,
) {
    sealed interface Location {
        data object Unknown : Location
        data object Onboarding : Location
        data class InRecycler(
            val recyclerIndex: RecyclerIndex,
            /**
             * Keys baked into the ring's root, which is the set a proof actually verifies against — so it is
             * the anonymity this voucher really has, not how many keys the ring has been offered.
             */
            val recyclerMembers: Int,
            /**
             * When the app first confirmed this voucher in the ring root.
             * Null until confirmed. Saved across restarts and reset on restore.
             */
            val enteredAt: Instant?,
        ) : Location
    }
}

fun RecyclerVoucher.tokenAmount() = recyclerValue.tokenAmount()

fun RecyclerVoucher.isInRecycler() = location is RecyclerVoucher.Location.InRecycler

fun RecyclerVoucher.recyclerLocationOrThrow() = location as RecyclerVoucher.Location.InRecycler

/** A voucher outside a recycler hides in nothing, which is the same as an empty ring. */
fun RecyclerVoucher.recyclerMembersOrZero() =
    (location as? RecyclerVoucher.Location.InRecycler)?.recyclerMembers ?: 0

fun List<RecyclerVoucher>.filterInRecycler(): List<RecyclerVoucher> {
    return filter { it.isInRecycler() }
}
