package io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption

import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.domain.model.X25519SharedSecret
import org.bouncycastle.crypto.digests.SHA256Digest
import org.bouncycastle.crypto.generators.HKDFBytesGenerator
import org.bouncycastle.crypto.params.HKDFParameters

// Empty salt/info: the X25519 shared secret has full entropy and the caller provides domain separation.
internal fun hkdfSha256(sharedSecret: X25519SharedSecret): AeadKey {
    val hkdf = HKDFBytesGenerator(SHA256Digest())
    hkdf.init(HKDFParameters(sharedSecret.bytes.value, byteArrayOf(), byteArrayOf()))

    val out = ByteArray(AeadKey.SIZE_BYTES)
    hkdf.generateBytes(out, 0, AeadKey.SIZE_BYTES)

    return AeadKey.fromDerivedBytes(out)
}
