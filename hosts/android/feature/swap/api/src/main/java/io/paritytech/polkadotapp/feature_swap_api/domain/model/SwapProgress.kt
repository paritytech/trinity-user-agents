package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

sealed class SwapProgress {
    class SegmentStarted(val step: SwapProgressStep) : SwapProgress()

    object ToRecipientTransferStarted : SwapProgress()

    class SegmentFailure(val error: Throwable, val attemptedStep: SwapProgressStep) : SwapProgress()

    class TransferFailure(val error: Throwable) : SwapProgress()

    class Done(val actualDeposited: Balance) : SwapProgress()
}

class SwapProgressStep(
    val index: Int,
    val operation: AtomicSwapOperation
)
