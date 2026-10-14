package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519SubstrateKeypairFactory
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.EncodedPrivateKey

typealias CoinPrivateKey = EncodedPrivateKey

fun CoinPrivateKey.deriveKeypair() = Sr25519SubstrateKeypairFactory.createKeypairFromSecret(value)

data class TransferMemo(
    val coins: List<TransferCoinEntry>,
    val totalValue: Balance
)

data class TransferCoinEntry(
    val privateKey: CoinPrivateKey,
    val valueExponent: ValueExponent
)
