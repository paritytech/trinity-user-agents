package io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair

class MetaAccountSecrets(
    val substrateKeyPair: Keypair,
    val entropy: ByteArray,
    val seed: ByteArray,
    val substrateDerivationPath: String?,
)
