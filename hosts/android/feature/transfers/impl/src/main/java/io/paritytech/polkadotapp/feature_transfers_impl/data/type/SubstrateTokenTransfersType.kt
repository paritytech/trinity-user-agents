package io.paritytech.polkadotapp.feature_transfers_impl.data.type

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicExecutionResult
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.ExtrinsicSubmission
import io.paritytech.polkadotapp.feature_transfers_api.data.type.TokenTransfersType
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.TransferArguments

abstract class SubstrateTokenTransfersType(
    private val extrinsicService: ExtrinsicService,
    private val chainRegistry: ChainRegistry,
    protected val chainAsset: Chain.Asset,
) : TokenTransfersType {
    protected abstract fun ExtrinsicBuilder.transfer(
        recipient: AccountId,
        amount: Balance,
    )

    override suspend fun calculateFee(args: TransferArguments): Result<AccountFee> {
        return extrinsicService.estimateFee(
            origin = args.origin,
            chain = chainRegistry.getChain(chainAsset.chainId),
            options = ExtrinsicService.SubmissionOptions(
                feePayment = args.feePayment
            )
        ) {
            transfer(args.recipient, args.amount)
        }
    }

    override suspend fun performAndTrackTransfer(args: TransferArguments): Result<ExtrinsicExecutionResult> {
        return extrinsicService.submitExtrinsicAndAwaitExecution(
            origin = args.origin,
            chain = chainRegistry.getChain(chainAsset.chainId),
            options = ExtrinsicService.SubmissionOptions(
                feePayment = args.feePayment
            ),
        ) {
            transfer(args.recipient, args.amount)
        }
    }

    override suspend fun submitTransfer(args: TransferArguments): Result<ExtrinsicSubmission> {
        return extrinsicService.submitExtrinsic(
            origin = args.origin,
            chain = chainRegistry.getChain(chainAsset.chainId),
            options = ExtrinsicService.SubmissionOptions(
                feePayment = args.feePayment
            ),
        ) {
            transfer(args.recipient, args.amount)
        }
    }
}
