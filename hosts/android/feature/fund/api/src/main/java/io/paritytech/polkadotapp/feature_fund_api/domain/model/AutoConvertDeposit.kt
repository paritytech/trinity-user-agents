package io.paritytech.polkadotapp.feature_fund_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import java.util.UUID

typealias DepositId = UUID

data class AutoConvertDeposit(
    val id: DepositId,
    val amount: ChainAssetWithAmount,
    val status: Status,
) {
    companion object {
        fun new(
            depositAmount: ChainAssetWithAmount,
            expectedConvertedAmount: ChainAssetWithAmount,
            completesAt: Timestamp
        ): AutoConvertDeposit {
            return AutoConvertDeposit(
                id = DepositId.randomUUID(),
                amount = depositAmount,
                status = Status.InProgress(completesAt, expectedConvertedAmount)
            )
        }
    }

    sealed class Status {
        data class InProgress(val completesAt: Timestamp, val expectedAmount: ChainAssetWithAmount) : Status()

        data class SwapCompleted(val actualConvertedAmount: ChainAssetWithAmount) : Status()

        data class Done(val actualAmount: ChainAssetWithAmount) : Status()

        data class Failure(val reason: Throwable) : Status()
    }

    fun updateProgress(newCompletesAt: Timestamp? = null): AutoConvertDeposit {
        require(status is Status.InProgress)

        return copy(status = Status.InProgress(newCompletesAt ?: status.completesAt, status.expectedAmount))
    }

    fun done(): AutoConvertDeposit {
        require(status is Status.InProgress)

        return copy(status = Status.Done(status.expectedAmount))
    }

    fun failed(error: Throwable): AutoConvertDeposit {
        require(status is Status.InProgress)

        return copy(status = Status.Failure(error))
    }
}
