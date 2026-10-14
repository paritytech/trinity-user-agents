package io.paritytech.polkadotapp.feature_transactions.api.data

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus

typealias MultiSubmissionResult = Result<List<Result<ExtrinsicStatus.InBlock>>>

inline fun MultiSubmissionResult.onIndividualSuccess(action: (index: Int, inBlock: ExtrinsicStatus.InBlock) -> Unit): MultiSubmissionResult {
    return onSuccess { individualResults ->
        individualResults.onEachIndexed { index, individualResult ->
            individualResult.onSuccess { action(index, it) }
        }
    }
}

inline fun MultiSubmissionResult.onAnyFailure(action: (throwable: Throwable) -> Unit): MultiSubmissionResult {
    return onFailure(action)
        .onSuccess { individualResults ->
            individualResults.onEach { individualResult ->
                individualResult.onFailure(action)
            }
        }
}

fun MultiSubmissionResult.requireAllIndividualSuccess(): Result<Unit> {
    return fold(
        onSuccess = { individualResults ->
            val firstFailure = individualResults.firstOrNull { it.isFailure }
            if (firstFailure != null) {
                val throwable = firstFailure.exceptionOrNull()!!
                Result.failure(throwable)
            } else {
                Result.success(Unit)
            }
        },
        onFailure = { Result.failure(it) }
    )
}
