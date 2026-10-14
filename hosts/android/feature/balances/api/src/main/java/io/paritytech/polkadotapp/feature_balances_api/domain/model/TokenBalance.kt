package io.paritytech.polkadotapp.feature_balances_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance

// TODO we can reduce interface surface by introducing
// withdrawable(preservation: BalancePreservation): Balance
// function which will replace transferable
interface TokenBalance {
    companion object;

    val token: Chain.Asset

    // Non-reserved part of the balance. There may still be restrictions on
    // this, but it is the total pool what may in principle be transferred,
    // reserved.
    val free: Balance

    // Balance which is reserved and may not be used at all.
    // This balance is a 'reserve' balance that different subsystems use in
    // order to set aside tokens that are still 'owned' by the account
    // holder, but which are suspendable
    val reserved: Balance

    // The amount that `free` may not drop below when withdrawing.
    val frozen: Balance

    // Amount that can be transferred
    val transferable: Balance

    // Balance counted towards Existential Deposit
    // If it drops below Existential Deposit, the account balance will be reaped
    val balanceCountedTowardsED: Balance

    val total: Balance

    fun reservable(existentialDeposit: Balance): Balance

    /**
     * Whether it is possible to reserve [amount] given current balance
     */
    fun canReserve(amount: Balance): Boolean
}
