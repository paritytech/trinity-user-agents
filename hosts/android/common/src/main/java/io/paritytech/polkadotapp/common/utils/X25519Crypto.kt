package io.paritytech.polkadotapp.common.utils

import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.X25519SharedSecret
import org.bouncycastle.crypto.agreement.X25519Agreement
import org.bouncycastle.crypto.params.X25519PrivateKeyParameters
import org.bouncycastle.crypto.params.X25519PublicKeyParameters
import java.security.SecureRandom
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class X25519KeyGenerator @Inject constructor() {
    fun generateRandomKeypair(): X25519KeyPair {
        val privateKey = X25519PrivateKeyParameters(SecureRandom())

        return X25519KeyPair(
            privateKey = X25519PrivateKey.fromValidated(privateKey.encoded),
            publicKey = X25519PublicKey.fromValidated(privateKey.generatePublicKey().encoded),
        )
    }

    fun createKeyPair(privateKey: X25519PrivateKey): X25519KeyPair {
        val parameters = X25519PrivateKeyParameters(privateKey.bytes.value, 0)

        return X25519KeyPair(
            privateKey = privateKey,
            publicKey = X25519PublicKey.fromValidated(parameters.generatePublicKey().encoded),
        )
    }
}

/**
 * X25519 key agreement (RFC 7748).
 *
 * An all-zero output means the peer supplied a small-order point; RFC-0004 makes rejecting it
 * normative, so the agreement fails and the enclosing message must be rejected. BouncyCastle's
 * [X25519Agreement] already throws in that case, which this surfaces as a typed failure.
 */
fun x25519SharedSecret(
    localPrivateKey: X25519PrivateKey,
    remotePublicKey: X25519PublicKey,
): Result<X25519SharedSecret> = runCancellableCatching {
    val secret = ByteArray(X25519SharedSecret.SIZE_BYTES)

    X25519Agreement().apply {
        init(X25519PrivateKeyParameters(localPrivateKey.bytes.value, 0))
        calculateAgreement(X25519PublicKeyParameters(remotePublicKey.bytes.value, 0), secret, 0)
    }

    X25519SharedSecret.fromValidated(secret)
}
