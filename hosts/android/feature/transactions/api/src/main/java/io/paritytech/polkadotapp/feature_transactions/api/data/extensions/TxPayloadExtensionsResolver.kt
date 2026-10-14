package io.paritytech.polkadotapp.feature_transactions.api.data.extensions

import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService

interface TxPayloadExtensionsResolver {
    class Resolved(
        /**
         * Every extension from the input list with its `implicit` / `explicit`
         * bytes decoded via the matching runtime-metadata SCALE type. Order
         * matches the input.
         */
        val allRequestedExtensions: List<DecodedTransactionExtensionValue>,
        val submissionOptions: ExtrinsicService.SubmissionOptions,
        val customExtensions: List<TransactionExtension>,
    )

    /**
     * Decodes [extensions] against the runtime's extension set for [txExtVersion] and routes them
     * into [Resolved.submissionOptions] / [Resolved.customExtensions].
     *
     * [txExtVersion] > 0 forces V5 with that extension version. 0 falls back to the chain default
     * for V4/V5 selection while keeping extension version 0.
     */
    suspend fun resolve(
        extensions: List<EncodedTransactionExtensionValue>,
        txExtVersion: UByte,
        chainId: ChainId,
        isSigned: Boolean,
    ): Result<Resolved>
}
