package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.CurrentInstallationProvider
import kotlinx.coroutines.CompletableDeferred

class HeldInstallationGate : CurrentInstallationProvider {
    private val waiters = mutableListOf<CompletableDeferred<Result<CoinageInstallationId>>>()
    private var outcome: Result<CoinageInstallationId>? = null

    val waitingCallers: Int
        get() = waiters.count { !it.isCompleted }

    fun release(installation: CoinageInstallationId) = open(Result.success(installation))

    fun fail(error: Throwable) = open(Result.failure(error))

    fun releaseLatestCaller(installation: CoinageInstallationId) {
        waiters.last().complete(Result.success(installation))
    }

    override suspend fun awaitBackedUp(): Result<CoinageInstallationId> =
        outcome ?: CompletableDeferred<Result<CoinageInstallationId>>().also { waiters += it }.await()

    private fun open(result: Result<CoinageInstallationId>) {
        outcome = result
        waiters.forEach { it.complete(result) }
    }
}
