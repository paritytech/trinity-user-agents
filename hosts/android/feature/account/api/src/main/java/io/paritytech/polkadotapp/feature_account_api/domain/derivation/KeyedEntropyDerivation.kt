package io.paritytech.polkadotapp.feature_account_api.domain.derivation

import io.novasama.substrate_sdk_android.encrypt.junction.JunctionType
import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.paritytech.polkadotapp.common.utils.blake2b256

/**
 * RFC-0022 keyed-hash HDKD, shared by the ring-VRF and ECDH trees. Folds `hash(parent, chainCode)`
 * over the path's chain codes. Soft junctions have no meaning here and are rejected.
 */
fun deriveKeyedEntropy(root: ByteArray, path: String): ByteArray {
    val junctions = SubstrateJunctionDecoder.decode(path).junctions

    require(junctions.all { it.type == JunctionType.HARD }) {
        "Keyed-hash derivation only supports hard junctions, got: $path"
    }

    return junctions.fold(root) { parent, junction -> parent.blake2b256(key = junction.chaincode) }
}
