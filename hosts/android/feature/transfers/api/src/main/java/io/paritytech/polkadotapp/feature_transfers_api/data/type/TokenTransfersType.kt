package io.paritytech.polkadotapp.feature_transfers_api.data.type

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicExecutionResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.ExtrinsicSubmission
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.ParsedTransferCall
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.TransferArguments

interface TokenTransfersType {
    suspend fun calculateFee(args: TransferArguments): Result<AccountFee>

    suspend fun performAndTrackTransfer(args: TransferArguments): Result<ExtrinsicExecutionResult>

    suspend fun submitTransfer(args: TransferArguments): Result<ExtrinsicSubmission>

    suspend fun parseTransferCall(call: GenericCall.Instance): ParsedTransferCall?
}
