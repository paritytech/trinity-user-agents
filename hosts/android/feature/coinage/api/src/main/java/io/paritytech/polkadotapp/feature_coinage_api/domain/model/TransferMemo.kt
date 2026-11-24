package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519SubstrateKeypairFactory
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.Sr25519SecretKey
import io.paritytech.polkadotapp.common.domain.model.EncodedPrivateKey

typealias CoinPrivateKey = EncodedPrivateKey

fun CoinPrivateKey.deriveKeypair() = Sr25519SubstrateKeypairFactory.createKeypairFromSecret(value)

/** Null when the binding refuses this key; a binding that cannot load at all still throws. */
fun CoinPrivateKey.deriveKeypairOrNull(): Sr25519Keypair? {
    if (Sr25519SecretKey.fromBytes(this).isFailure) return null

    return try {
        deriveKeypair()
    } catch (_: Exception) {
        null
    }
}

data class TransferMemo(
    val coins: List<TransferCoinEntry>,
    val totalValue: Balance
)

data class TransferCoinEntry(
    val privateKey: CoinPrivateKey,
    val valueExponent: ValueExponent
)
