package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions

/** A transfer's transaction was proven unable to land before it could be handed over. */
class TransferSubmissionFailedException : Exception("Transfer transaction could not be submitted")
