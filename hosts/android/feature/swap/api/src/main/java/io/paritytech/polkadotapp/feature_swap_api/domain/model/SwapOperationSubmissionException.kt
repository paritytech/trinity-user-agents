package io.paritytech.polkadotapp.feature_swap_api.domain.model

sealed class SwapOperationSubmissionException : Throwable() {
    class SimulationFailed : SwapOperationSubmissionException()
}
