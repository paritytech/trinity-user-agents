package io.paritytech.polkadotapp.feature_transactions_impl.data.validation

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.DataByteArray

/**
 * SCALE-encoded extrinsic body without the outer `Vec<u8>` length prefix — the bytes of
 * `SendableExtrinsic.bytesWithoutLength`.
 */
typealias EncodedExtrinsicBody = DataByteArray

interface ExtrinsicValidator {
    /**
     * Validates [extrinsic] against the runtime state of [chainId] at [atBlockHash] by calling the
     * `TaggedTransactionQueue_validate_transaction` runtime API.
     *
     * @return the [TransactionValidity] verdict on success; [Result.failure] only if the runtime call itself
     *  fails (network or decoding error) — an on-chain `Invalid`/`Unknown` verdict is a successful result.
     */
    suspend fun validate(
        chainId: ChainId,
        extrinsic: EncodedExtrinsicBody,
        atBlockHash: BlockHash,
    ): Result<TransactionValidity>
}
