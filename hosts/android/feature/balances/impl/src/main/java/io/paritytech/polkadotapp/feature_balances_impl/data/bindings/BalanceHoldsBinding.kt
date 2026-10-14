package io.paritytech.polkadotapp.feature_balances_impl.data.bindings

import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.feature_balances_api.domain.model.BalanceHold
import io.paritytech.polkadotapp.feature_balances_api.domain.model.BalanceHoldId

fun bindBalanceHolds(dynamic: Any?): List<BalanceHold> {
    return bindList(dynamic) {
        val struct = it.castToStruct()
        val id = struct.get<Any>("id").castToDictEnum()
        val module = id.name
        val reason = id.value.castToDictEnum().name
        BalanceHold(
            BalanceHoldId(module, reason),
            bindBalance(struct["amount"])
        )
    }
}
