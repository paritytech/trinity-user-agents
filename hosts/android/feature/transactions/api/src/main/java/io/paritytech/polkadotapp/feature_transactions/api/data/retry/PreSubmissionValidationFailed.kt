package io.paritytech.polkadotapp.feature_transactions.api.data.retry

/**
 * Pre-submission validation rejected the extrinsic, so it was never handed to a node.
 *
 * Carried by [io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus.FailedToSubmit], and part of the contract
 * rather than an implementation detail: it is the one submission failure that proves the bytes never reached
 * a pool, which is what lets a caller treat the transaction as dead instead of waiting out its mortality.
 */
class PreSubmissionValidationFailed() : Exception("Extrinsic was rejected as invalid before submission")
