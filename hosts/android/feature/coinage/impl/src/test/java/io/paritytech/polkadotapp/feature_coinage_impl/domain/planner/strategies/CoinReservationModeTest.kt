package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class CoinReservationModeTest {
    private val transactionService: CoinageTransactionService = mock()
    private val handoffCommit: CoinageHandoffCommit = mock()
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

    private suspend fun withHandoffAccepted() {
        whenever(transactionService.prepareHandoff(any())).thenReturn(Result.success(handoffCommit))
        whenever(transactionService.preCommitHandoff(any())).thenReturn(Result.success(handoffCommit))
    }

    private suspend fun verifyHandoffPrepared() {
        verify(transactionService).prepareHandoff(eq(assets))
        verify(transactionService, never()).preCommitHandoff(any())
    }

    private suspend fun verifyHandoffPreCommitted() {
        verify(transactionService).preCommitHandoff(eq(assets))
        verify(transactionService, never()).prepareHandoff(any())
    }
}
