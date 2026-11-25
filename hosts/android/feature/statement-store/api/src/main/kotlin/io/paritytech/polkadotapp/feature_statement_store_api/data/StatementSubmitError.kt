package io.paritytech.polkadotapp.feature_statement_store_api.data

sealed class StatementSubmitError(message: String) : Throwable(message) {
    class NoAllowance : StatementSubmitError("The signer has no statement allowance")

    class ChannelPriorityTooLow : StatementSubmitError("The channel already holds a statement with an equal or higher expiry")

    class Rejected(result: String) : StatementSubmitError("The store rejected the statement for good: $result")
}
