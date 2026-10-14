package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetStates
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock

/**
 * The join between a local asset row and the ledger's claim on it.
 *
 * The row alone says whether the chain holds the asset; the ledger alone says whether a transaction of ours
 * has a claim on it. Balance, coin selection and payment status all read through this, so a join that pairs
 * the wrong two halves, or stops re-reading one of them, is wrong everywhere at once.
 */
class RealCoinageAssetsUseCaseTest {
    private val coinRepository: CoinRepository = mock()
    private val voucherRepository: VoucherRepository = mock()
    private val transactionService: CoinageTransactionService = mock()

    private val useCase = RealCoinageAssetsUseCase(coinRepository, voucherRepository, transactionService)

    @Test
    fun `a coin is paired with the ledger's claim on it`() = runBlocking {
        val coin = coinOf(derivationIndex = 7, age = 1)
        val claimed = CoinageAssetState(handedOff = false, minterStatus = FINALIZED_SUCCESS, consumerStatus = PENDING)

        givenCoins(coin)
        givenStates(OwnAsset.Coin(testKey(7)) to claimed)

        assertEquals(listOf(claimed), useCase.subscribeCoins().first().map { it.state })
    }

    /**
     * The ledger only holds assets some transaction of ours has touched, so most coins are absent from it.
     * Absence has to read as "no claim" rather than as missing data — otherwise a wallet's untouched coins
     * would drop out of the balance entirely.
     */
    @Test
    fun `a coin the ledger has never heard of carries no claim`() = runBlocking {
        givenCoins(coinOf(derivationIndex = 7, age = 1))
        givenStates()

        assertEquals(listOf(CoinageAssetState.UNTRACKED), useCase.subscribeCoins().first().map { it.state })
    }

    /**
     * A coin is joined to its state by derivation index, and a voucher by ring key index. Both are plain
     * Ints, so a join that mixed the two up would still typecheck and would still find a state — the wrong
     * one — whenever the numbers happened to coincide.
     */
    @Test
    fun `a coin does not take the state of the voucher sharing its index`() = runBlocking {
        val voucherState = CoinageAssetState(handedOff = true, minterStatus = null, consumerStatus = null)

        givenCoins(coinOf(derivationIndex = 3, age = 1))
        givenVouchers(voucherOf(ringVrfKeyIndex = 3, location = Location.Onboarding))
        givenStates(OwnAsset.Voucher(testKey(3)) to voucherState)

        assertEquals(listOf(CoinageAssetState.UNTRACKED), useCase.subscribeCoins().first().map { it.state })
        assertEquals(listOf(voucherState), useCase.subscribeVouchers().first().map { it.state })
    }

    /**
     * The ledger changes far more often than the local rows do: a transaction being registered, included or
     * failed moves a claim without the coin itself changing at all. If the join only re-read on a row change,
     * a coin would stay spendable in the UI for as long as nothing else happened to it.
     */
    @Test
    fun `a claim appearing on an unchanged coin is re-reported`() = runBlocking {
        val claimed = CoinageAssetState(handedOff = false, minterStatus = null, consumerStatus = PENDING)

        givenCoins(coinOf(derivationIndex = 7, age = 1))
        whenever(transactionService.subscribeAssetStates())
            .thenReturn(flowOf(statesOf(), statesOf(OwnAsset.Coin(testKey(7)) to claimed)))

        val reported = useCase.subscribeCoins().toList().map { tracked -> tracked.map { it.state } }

        assertEquals(listOf(listOf(CoinageAssetState.UNTRACKED), listOf(claimed)), reported)
    }

    @Test
    fun `only the coins of the addresses asked for are joined`() = runBlocking {
        val accountId: AccountId = mock()
        val coin = coinOf(derivationIndex = 7, age = 1)

        whenever(coinRepository.subscribeCoinsBy(listOf(accountId))).thenReturn(flowOf(listOf(coin)))
        givenStates()

        assertEquals(listOf(coin), useCase.subscribeCoinsBy(listOf(accountId)).first().map { it.coin })
    }

    /**
     * Selectability needs both halves to agree: the chain must hold the coin, the ledger must have no claim
     * on it, and it must not be so old that it is due for recycling.
     */

    /**
     * Room invalidates a query when its *table* is written, not when the rows it selected change. The
     * presence sync writes every coin in the wallet and the ledger writes on every status change, so this
     * join is re-run constantly with identical results.
     *
     * Callers act on what it emits — the payment status reads the finalized chain per emission — so passing
     * those through would turn one background write into a chain read for every payment on screen.
     */
    @Test
    fun `a join that resolves to the same thing again is not re-emitted`() = runBlocking {
        val coin = coinOf(derivationIndex = 7, age = 1)
        val state = CoinageAssetState(handedOff = false, minterStatus = FINALIZED_SUCCESS, consumerStatus = null)

        // The same rows read three times over, as an unrelated write to either table would produce.
        whenever(coinRepository.subscribeCoinsBy(any())).thenReturn(flowOf(listOf(coin), listOf(coin), listOf(coin)))
        whenever(transactionService.subscribeAssetStates())
            .thenReturn(flowOf(statesOf(OwnAsset.Coin(testKey(7)) to state)))

        val emissions = useCase.subscribeCoinsBy(listOf(coin.accountId)).toList()

        assertEquals(1, emissions.size)
    }

    private suspend fun givenCoins(vararg coins: Coin) {
        whenever(coinRepository.subscribeAllCoins()).thenReturn(flowOf(coins.toList()))
        whenever(coinRepository.getCoinRecyclingAge()).thenReturn(Result.success(RECYCLING_AGE))
    }

    private fun givenVouchers(vararg vouchers: RecyclerVoucher) {
        whenever(voucherRepository.subscribeAllVouchers()).thenReturn(flowOf(vouchers.toList()))
    }

    private fun givenStates(vararg states: Pair<OwnAsset, CoinageAssetState>) {
        whenever(transactionService.subscribeAssetStates()).thenReturn(flowOf(statesOf(*states)))
    }

    private fun statesOf(vararg states: Pair<OwnAsset, CoinageAssetState>) = CoinageAssetStates(states.toMap(), TEST_INSTALLATION)

    private fun coinOf(derivationIndex: Int, age: Int?) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = age?.let(Coin.Age::Known) ?: Coin.Age.Unknown,
        isOnChain = age != null,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private fun voucherOf(ringVrfKeyIndex: Int, location: Location) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(ringVrfKeyIndex),
        ringVrfPublicKey = mock(),
        recyclerValue = ValueExponent(1),
        location = location,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private companion object {
        const val RECYCLING_AGE = 14
    }
}
