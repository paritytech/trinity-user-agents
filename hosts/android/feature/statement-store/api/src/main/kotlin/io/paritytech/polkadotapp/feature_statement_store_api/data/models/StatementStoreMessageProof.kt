package io.paritytech.polkadotapp.feature_statement_store_api.data.models

import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper

class StatementStoreMessageProof(
    val signature: SignatureWrapper.Sr25519,
    val publicKey: ByteArray
)
