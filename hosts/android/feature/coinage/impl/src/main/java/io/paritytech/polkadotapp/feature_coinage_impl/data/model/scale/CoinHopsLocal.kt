package io.paritytech.polkadotapp.feature_coinage_impl.data.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

/**
 * The on-disk shape of a coin's hop history, stored SCALE-encoded in `coins.hops`.
 *
 * Kept apart from the domain [io.paritytech.polkadotapp.feature_coinage_api.domain.model.Hop] so the domain
 * stays free to change: SCALE is positional, so any reorder or retype here corrupts every existing row and
 * needs a `CoinHopsLocalV<N>` snapshot plus a migration. See `code/database-and-scale.md`.
 *
 * The counts are `Int` rather than a byte type: the codec has no unsigned-byte binding, and this blob never
 * leaves the device, so the extra width costs three bytes a hop and buys compile-time safety. The `0..255`
 * range the counts are specified over is enforced where they are built, not by the encoding.
 */
@Serializable
class CoinHopsLocal(val hops: List<CoinHopLocal>)

@Serializable
sealed class CoinHopLocal {
    @Serializable
    @EnumIndex(0)
    class Transfer(val bundleSize: Int) : CoinHopLocal()

    @Serializable
    @EnumIndex(1)
    class Split(val fanout: Int) : CoinHopLocal()
}
