package io.paritytech.polkadotapp.feature_chats_impl.data.hop.encryption

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import javax.inject.Inject

class HopTicketKeyDerivation @Inject constructor() {
    fun deriveSigningKeyPair(ticket: HopTicket): Sr25519Keypair {
        val seed = SIGNER_CONTEXT.blake2b256(key = ticket.bytes)
        return SubstrateKeypairFactory.generate(EncryptionType.SR25519, seed) as Sr25519Keypair
    }

    fun deriveEncryptionKey(ticket: HopTicket): AeadKey {
        return AeadKey.fromDerivedBytes(ENCRYPTION_CONTEXT.blake2b256(key = ticket.bytes))
    }

    companion object {
        private val SIGNER_CONTEXT = "signer".encodeToByteArray()
        private val ENCRYPTION_CONTEXT = "encryption".encodeToByteArray()
    }
}
