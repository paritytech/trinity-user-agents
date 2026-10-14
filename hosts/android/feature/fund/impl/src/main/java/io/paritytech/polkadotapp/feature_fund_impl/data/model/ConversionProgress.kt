package io.paritytech.polkadotapp.feature_fund_impl.data.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapProgressStep

sealed class ConversionProgress {
    class SwapInProgress(val swapProgressStep: SwapProgressStep) : ConversionProgress()

    object TransferToRecipient : ConversionProgress()

    class Done(val actualDeposited: Balance) : ConversionProgress()

    class Failed(val error: Throwable) : ConversionProgress()
}
