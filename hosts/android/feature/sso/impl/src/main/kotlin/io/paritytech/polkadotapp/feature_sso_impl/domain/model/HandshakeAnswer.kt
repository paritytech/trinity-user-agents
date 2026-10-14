package io.paritytech.polkadotapp.feature_sso_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

class HandshakeAnswer(
    val encryptedData: ByteArray,
    val tempSharedEncryptionPublicKey: X25519PublicKey
)
