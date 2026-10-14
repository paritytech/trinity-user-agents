package io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType

import io.paritytech.polkadotapp.chains.network.binding.AccountData
import io.paritytech.polkadotapp.chains.network.binding.AccountInfo
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.TransferableMode

internal fun AccountInfo.transferableBalance(): Balance {
    return transferableMode.transferableBalance(data)
}

internal val AccountInfo.transferableMode: TransferableMode
    get() = if (data.flags.holdsAndFreezesEnabled()) {
        TransferableMode.HOLDS_AND_FREEZES
    } else {
        TransferableMode.LEGACY
    }

private fun TransferableMode.transferableBalance(accountBalance: AccountData): Balance {
    return transferableBalance(accountBalance.free, accountBalance.frozen, accountBalance.reserved)
}
