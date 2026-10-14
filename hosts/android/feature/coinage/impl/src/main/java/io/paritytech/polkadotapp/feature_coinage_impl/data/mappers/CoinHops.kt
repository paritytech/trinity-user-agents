package io.paritytech.polkadotapp.feature_coinage_impl.data.mappers

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.utils.decodeFromByteArrayCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Hop
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.scale.CoinHopLocal
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.scale.CoinHopsLocal
import kotlinx.serialization.encodeToByteArray

/** Absent and empty both read back as no hops; an unreadable blob is a failure the caller decides about. */
fun ByteArray?.decodeCoinHops(): Result<List<Hop>> {
    if (this == null || isEmpty()) return Result.success(emptyList())

    return BinaryScale.decodeFromByteArrayCatching<CoinHopsLocal>(this)
        .map { local -> local.hops.map { it.toDomain() } }
}

fun List<Hop>.encodeCoinHops(): ByteArray =
    BinaryScale.encodeToByteArray(CoinHopsLocal(map { it.toLocal() }))

private fun CoinHopLocal.toDomain(): Hop = when (this) {
    is CoinHopLocal.Transfer -> Hop.Transfer.of(bundleSize)
    is CoinHopLocal.Split -> Hop.Split.of(fanout)
}

private fun Hop.toLocal(): CoinHopLocal = when (this) {
    is Hop.Transfer -> CoinHopLocal.Transfer(bundleSize)
    is Hop.Split -> CoinHopLocal.Split(fanout)
}
