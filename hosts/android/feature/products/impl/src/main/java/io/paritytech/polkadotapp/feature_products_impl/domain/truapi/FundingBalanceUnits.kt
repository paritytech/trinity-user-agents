package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import uniffi.truapi.U128

/** An amount as the core sends it: the decimal string of the asset's smallest unit. `null` when it is not a number. */
internal fun U128.toBalanceOrNull(): Balance? = toBigIntegerOrNull()?.intoBalance()

/** The amount as the core takes it. */
internal fun Balance.toU128(): U128 = value.toString()
