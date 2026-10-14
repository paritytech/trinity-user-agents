package io.paritytech.polkadotapp.chains.util.signing

import io.paritytech.polkadotapp.common.utils.endsWith
import io.paritytech.polkadotapp.common.utils.startsWith

fun interface MessageSigningContext {
    companion object {
        fun generalUntrustedMessage(): MessageSigningContext {
            return GeneralUntrustedMessage(
                prefix = "<Bytes>".encodeToByteArray(),
                suffix = "</Bytes>".encodeToByteArray()
            )
        }

        fun trustedContent(): MessageSigningContext {
            return MessageSigningContext { it }
        }
    }

    fun messageInContext(unsafeMessage: ByteArray): ByteArray
}

class GeneralUntrustedMessage(
    private val prefix: ByteArray,
    private val suffix: ByteArray
) : MessageSigningContext {
    override fun messageInContext(unsafeMessage: ByteArray): ByteArray {
        if (unsafeMessage.startsWith(prefix) && unsafeMessage.endsWith(suffix)) {
            return unsafeMessage
        }

        return prefix + unsafeMessage + suffix
    }
}
