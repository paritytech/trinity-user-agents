package io.paritytech.polkadotapp.tools_car_parser

import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.cbor.Cbor

private const val IDENTITY_MULTIBASE_PREFIX: Byte = 0x00

/**
 * Decodes CARv1 headers using kotlinx-serialization-cbor.
 * The header is a CBOR map: { "version": 1, "roots": [<CID bytes>, ...] }
 */
object CborHeaderDecoder {
    data class CarHeader(
        val version: Int,
        val roots: List<Cid>
    )

    @OptIn(ExperimentalSerializationApi::class)
    @Serializable
    @JvmInline
    private value class CborByteString(@ByteString val value: ByteArray)

    @Serializable
    private data class RawCarHeader(
        val version: Int,
        val roots: List<CborByteString>
    )

    @OptIn(ExperimentalSerializationApi::class)
    private val cbor = Cbor {
        ignoreUnknownKeys = true
    }

    @OptIn(ExperimentalSerializationApi::class)
    fun decode(headerBytes: ByteArray): CarHeader {
        val raw = cbor.decodeFromByteArray(RawCarHeader.serializer(), headerBytes)

        require(raw.version == 1) { "Unsupported CAR version: ${raw.version}" }
        require(raw.roots.isNotEmpty()) { "CAR header missing roots" }

        val roots = raw.roots.map { wrapper ->
            val cidBytes = wrapper.value
            // Strip the 0x00 identity multibase prefix required by DAG-CBOR link encoding
            val rawCid = if (cidBytes.isNotEmpty() && cidBytes[0] == IDENTITY_MULTIBASE_PREFIX) {
                cidBytes.copyOfRange(1, cidBytes.size)
            } else {
                cidBytes
            }
            Cid.cast(rawCid)
        }

        return CarHeader(version = raw.version, roots = roots)
    }
}
