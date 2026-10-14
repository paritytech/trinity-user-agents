package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarBalanceStatus
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import org.junit.Assert.assertEquals
import org.junit.Test

class DigitalDollarBalanceStatusTest {
    @Test
    fun `nothing clearing shows only the total`() {
        val card = cardOf(balance = 30, ready = 30)

        assertEquals(DigitalDollarBalanceStatus.TotalOnly, card.balanceStatus)
    }

    @Test
    fun `something clearing shows the ready line`() {
        val ready = TokenAmountModel.mock(20)
        val card = cardOf(amounts = LoadingState.Loaded(amountsOf(balance = TokenAmountModel.mock(30), ready = ready)))

        assertEquals(DigitalDollarBalanceStatus.PartlyReady(ready), card.balanceStatus)
    }

    @Test
    fun `syncing outranks the ready line`() {
        val card = cardOf(balance = 30, ready = 20, syncInProgress = true)

        assertEquals(DigitalDollarBalanceStatus.Syncing, card.balanceStatus)
    }

    @Test
    fun `syncing outranks a pending account backup`() {
        val card = cardOf(balance = 30, ready = 20, syncInProgress = true, accountBackupPending = true)

        assertEquals(DigitalDollarBalanceStatus.Syncing, card.balanceStatus)
    }

    @Test
    fun `a pending account backup outranks the ready line`() {
        val card = cardOf(balance = 30, ready = 20, accountBackupPending = true)

        assertEquals(DigitalDollarBalanceStatus.AccountBackupPending, card.balanceStatus)
    }

    @Test
    fun `amounts still loading show only the total`() {
        val card = cardOf(amounts = LoadingState.Loading)

        assertEquals(DigitalDollarBalanceStatus.TotalOnly, card.balanceStatus)
    }

    private fun cardOf(
        balance: Int,
        ready: Int,
        syncInProgress: Boolean = false,
        accountBackupPending: Boolean = false,
    ) = cardOf(
        amounts = LoadingState.Loaded(amountsOf(TokenAmountModel.mock(balance), TokenAmountModel.mock(ready))),
        syncInProgress = syncInProgress,
        accountBackupPending = accountBackupPending,
    )

    private fun cardOf(
        amounts: LoadingState<PocketCardUiModel.DigitalDollar.Amounts>,
        syncInProgress: Boolean = false,
        accountBackupPending: Boolean = false,
    ) = PocketCardUiModel.DigitalDollar(
        amounts = amounts,
        syncInProgress = syncInProgress,
        accountBackupPending = accountBackupPending,
    )

    private fun amountsOf(balance: TokenAmountModel, ready: TokenAmountModel) =
        PocketCardUiModel.DigitalDollar.Amounts(balance = balance, ready = ready)
}
