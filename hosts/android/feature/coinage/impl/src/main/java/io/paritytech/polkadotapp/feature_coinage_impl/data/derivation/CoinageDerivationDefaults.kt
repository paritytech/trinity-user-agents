package io.paritytech.polkadotapp.feature_coinage_impl.data.derivation

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex

object CoinageDerivationDefaults {
    // RFC-0017 shapes coinage derivations as `<root>//<purse>//<page>//<item>`. We only ever derive into the main purse,
    // which the RFC pins to `u32::MAX` so that the randomly assigned product-purse ids can never collide with it.
    const val COINAGE_MAIN_PURSE_INDEX = 4_294_967_295L
}

// Hex keeps the 32 bytes intact: the junction decoder maps a hex segment straight to the raw chain code, where any
// other encoding would be hashed or length-prefixed first.
fun CoinageInstallationId.asPageSegment(): String = value.value.toHexString(withPrefix = true)

// Derives every index against its own installation's base, one base per installation, in input order.
internal inline fun <B, R> List<CoinageKeyIndex>.deriveGroupedByInstallation(
    base: (CoinageInstallationId) -> B,
    child: (B, Int) -> R,
): List<R> {
    val bases = map { it.installation }.distinct().associateWith(base)

    return map { child(bases.getValue(it.installation), it.item) }
}
