package io.paritytech.polkadotapp.feature_chats_impl.data.hop.encryption

import io.paritytech.polkadotapp.common.data.encryption.MessageEncryption
import io.paritytech.polkadotapp.common.data.encryption.chaCha20Poly1305
import io.paritytech.polkadotapp.common.domain.model.AeadKey

// The ticket-derived key is already a 32-byte keyed hash, so it feeds the AEAD with no further derivation.
class HopEncryption(key: AeadKey) : MessageEncryption by MessageEncryption.chaCha20Poly1305(key)
