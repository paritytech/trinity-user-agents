package io.paritytech.polkadotapp.feature_chats_impl.data.hop.download

import io.ipfs.multihash.Multihash
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import io.paritytech.polkadotapp.tools_ipfs_api.Cids

internal fun Cids.hopBitswapCid(hash: ByteArray): String {
    return Cid.buildCidV1(Cid.Codec.Raw, Multihash.Type.blake2b_256, hash).toString()
}
