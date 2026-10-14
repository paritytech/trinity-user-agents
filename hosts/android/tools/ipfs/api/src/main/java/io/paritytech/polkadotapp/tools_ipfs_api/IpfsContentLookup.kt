package io.paritytech.polkadotapp.tools_ipfs_api

import io.ipfs.multihash.Multihash

interface IpfsContentLookup {
    suspend fun getIpfsLinkFor(hash: Multihash, codec: Cid.Codec): Result<String>
    suspend fun getIpfsLinkFor(cid: String): Result<String>

    suspend fun lookupRawHash(hash: ByteArray): Result<ByteArray>
}

suspend fun IpfsContentLookup.getIpfsLinkFor(cid: Cid): Result<String> {
    return getIpfsLinkFor(cid.toString())
}

suspend fun IpfsContentLookup.getDefaultJsonLink(hash: ByteArray): Result<String> {
    return getIpfsLinkFor(Multihash(Multihash.Type.blake2b_256, hash), Cid.Codec.Json)
}

suspend fun IpfsContentLookup.getDefaultRawLink(hash: ByteArray): Result<String> {
    return getIpfsLinkFor(Multihash(Multihash.Type.blake2b_256, hash), Cid.Codec.Raw)
}
