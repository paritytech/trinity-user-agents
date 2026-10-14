package io.paritytech.polkadotapp.feature_transactions_impl.data.validation

/**
 * Outcome of validating an extrinsic against the transaction pool via `TaggedTransactionQueue`.
 */
sealed interface TransactionValidity {
    data object Valid : TransactionValidity

    data class Invalid(val reason: InvalidTransaction) : TransactionValidity {
        /**
         * Whether the extrinsic can no longer become valid by waiting — its mortality window has passed.
         */
        val isMortalityExpired: Boolean
            get() = reason is InvalidTransaction.Stale || reason is InvalidTransaction.AncientBirthBlock
    }

    data class Unknown(val reason: UnknownTransaction) : TransactionValidity
}
