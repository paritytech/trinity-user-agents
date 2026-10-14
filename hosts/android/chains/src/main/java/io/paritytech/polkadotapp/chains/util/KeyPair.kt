package io.paritytech.polkadotapp.chains.util

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.encrypt.Sr25519
import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.novasama.substrate_sdk_android.encrypt.keypair.BaseKeypair
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.seed.substrate.SubstrateSeedFactory
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.MultiSignature
import io.novasama.substrate_sdk_android.scale.EncodableStruct
import io.novasama.substrate_sdk_android.scale.Schema
import io.novasama.substrate_sdk_android.scale.byteArray
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import java.security.SecureRandom

object KeyPairSchema : Schema<KeyPairSchema>() {
    val PrivateKey by byteArray()
    val PublicKey by byteArray()

    val Nonce by byteArray().optional()
}

fun Keypair.toStruct(): EncodableStruct<KeyPairSchema> {
    return KeyPairSchema { keypair ->
        keypair[PublicKey] = publicKey
        keypair[PrivateKey] = privateKey
        keypair[Nonce] = (this@toStruct as? Sr25519Keypair)?.nonce
    }
}

fun EncodableStruct<KeyPairSchema>.toKeypair(): Keypair {
    return createKeypair(
        publicKey = get(KeyPairSchema.PublicKey),
        privateKey = get(KeyPairSchema.PrivateKey),
        nonce = get(KeyPairSchema.Nonce)
    )
}

fun EncodableStruct<KeyPairSchema>.toSr25519Keypair(): Sr25519Keypair {
    return Sr25519Keypair(
        publicKey = get(KeyPairSchema.PublicKey),
        privateKey = get(KeyPairSchema.PrivateKey),
        nonce = requireNotNull(get(KeyPairSchema.Nonce))
    )
}

fun Sr25519Keypair.sign(message: ByteArray, context: MessageSigningContext): ByteArray {
    return Sr25519.sign(publicKey, privateKey + nonce, context.messageInContext(message))
}

fun Sr25519Keypair.signIntoWrapper(message: ByteArray, context: MessageSigningContext): SignatureWrapper.Sr25519 {
    return SignatureWrapper.Sr25519(sign(message, context))
}

fun Sr25519Keypair.signMultiSignature(message: ByteArray, context: MessageSigningContext): MultiSignature {
    return MultiSignature(EncryptionType.SR25519, sign(message, context))
}

fun createKeypair(
    publicKey: ByteArray,
    privateKey: ByteArray,
    nonce: ByteArray? = null,
) = if (nonce != null) {
    Sr25519Keypair(
        publicKey = publicKey,
        privateKey = privateKey,
        nonce = nonce
    )
} else {
    BaseKeypair(
        privateKey = privateKey,
        publicKey = publicKey
    )
}

object KeyPairGenerator {
    fun randomEntropy(length: Mnemonic.Length): ByteArray {
        val entropy = ByteArray(length.byteLength)
        SecureRandom().nextBytes(entropy)

        return entropy
    }

    fun deriveSr25519From(
        mnemonic: Mnemonic,
        derivationPath: String? = null
    ): Sr25519Keypair {
        val decodedPath = derivationPath?.let(SubstrateJunctionDecoder::decode)

        val seed = SubstrateSeedFactory.deriveSeed32(mnemonic.words, password = decodedPath?.password)
        val keypair = SubstrateKeypairFactory.generate(EncryptionType.SR25519, seed.seed, decodedPath?.junctions.orEmpty())

        return keypair as Sr25519Keypair
    }
}
