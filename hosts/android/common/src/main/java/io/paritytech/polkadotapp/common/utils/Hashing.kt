package io.paritytech.polkadotapp.common.utils

import org.bouncycastle.crypto.digests.Blake2bDigest
import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256 as substrateBlake2b256

fun ByteArray.blake2b256(key: ByteArray? = null): ByteArray {
    return if (key != null) {
        val digest = Blake2bDigest(key, 32, null, null)

        digest.update(this, 0, this.size)

        val result = ByteArray(digest.digestSize)
        digest.doFinal(result, 0)

        result
    } else {
        substrateBlake2b256()
    }
}
