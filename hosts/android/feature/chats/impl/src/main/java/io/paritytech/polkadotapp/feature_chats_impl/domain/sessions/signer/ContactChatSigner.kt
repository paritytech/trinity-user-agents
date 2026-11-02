package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair

/** [period] is the slot period the signer was resolved in. */
class ContactChatSigner(
    val keypair: Sr25519Keypair,
    val kind: ChatSignerKind,
    val period: UInt,
)

enum class ChatSignerKind { PRIVATE, USERNAME }
