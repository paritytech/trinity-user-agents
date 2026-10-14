package io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

sealed class SwapLimit {
    data class SpecifiedIn(
        val amountIn: Balance,
        val amountOutMin: Balance
    ) : SwapLimit()

    data class SpecifiedOut(
        val amountOut: Balance,
        val amountInMax: Balance
    ) : SwapLimit()
}
