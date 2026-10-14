package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.chains.util.Sr25519SecretKey
import io.paritytech.polkadotapp.chains.util.deriveKeypair
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray

/**
 * Expanded sr25519 secret of an allowance slot account. Public key is derived deterministically when needed.
 */
typealias SlotAccountKey = Sr25519SecretKey

fun SlotAccountKey.deriveAccountId(): AccountId = deriveKeypair().publicKey.toDataByteArray()
