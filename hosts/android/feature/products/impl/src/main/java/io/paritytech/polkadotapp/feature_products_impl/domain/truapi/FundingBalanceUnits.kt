package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import uniffi.truapi.U128

internal fun U128.toBalanceOrNull(): Balance? = toBigIntegerOrNull()?.intoBalance()

internal fun Balance.toU128(): U128 = value.toString()
