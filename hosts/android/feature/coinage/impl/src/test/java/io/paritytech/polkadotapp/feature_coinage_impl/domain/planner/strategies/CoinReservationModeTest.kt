package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.runBlocking
import org.junit.Test

class CoinReservationModeTest {
    private val transactionService: CoinageTransactionService = mockk()
    private val handoffCommit: CoinageHandoffCommit = mockk()
    private val assets = listOf(OwnAsset.Coin(testKey(1)))

    @Test
    fun `prepares the handoff when the mode is PREPARE`() = runBlocking<Unit> {
        withHandoffAccepted()

        transactionService.reserveAssets(assets, CoinReservationMode.PREPARE)

        verifyHandoffPrepared()
    }

    @Test
    fun `pre-commits the handoff when the mode is PRECOMMIT`() = runBlocking<Unit> {
        withHandoffAccepted()

        transactionService.reserveAssets(assets, CoinReservationMode.PRECOMMIT)

        verifyHandoffPreCommitted()
    }

    private fun withHandoffAccepted() {
        coEvery { transactionService.prepareHandoff(any()) } returns Result.success(handoffCommit)
        coEvery { transactionService.preCommitHandoff(any()) } returns Result.success(handoffCommit)
    }

    private fun verifyHandoffPrepared() {
        coVerify(exactly = 1) { transactionService.prepareHandoff(assets) }
        coVerify(exactly = 0) { transactionService.preCommitHandoff(any()) }
    }

    private fun verifyHandoffPreCommitted() {
        coVerify(exactly = 1) { transactionService.preCommitHandoff(assets) }
        coVerify(exactly = 0) { transactionService.prepareHandoff(any()) }
    }
}
