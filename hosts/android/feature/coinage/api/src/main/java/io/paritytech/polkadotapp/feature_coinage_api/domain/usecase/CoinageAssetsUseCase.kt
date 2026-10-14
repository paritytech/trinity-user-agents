package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import kotlinx.coroutines.flow.Flow

/**
 * The one place local asset rows and the ledger's view of them are joined.
 *
 * Neither half decides anything alone: the row says whether the chain holds the asset, the ledger says
 * whether a transaction of ours has a claim on it, and every question worth asking — is it spendable, is it
 * merely pending, has it gone for good — needs both. Balance, coin selection and payment status all read
 * through here so they cannot drift apart.
 */
interface CoinageAssetsUseCase {
    fun subscribeCoins(): Flow<List<TrackedCoin>>

    /** Only the coins at [accountIds] — for a caller watching one payment rather than the wallet. */
    fun subscribeCoinsBy(accountIds: List<AccountId>): Flow<List<TrackedCoin>>

    suspend fun getCoins(): List<TrackedCoin>

    fun subscribeVouchers(): Flow<List<TrackedVoucher>>

    suspend fun getVouchers(): List<TrackedVoucher>
}

data class TrackedCoin(
    val coin: Coin,
    val state: CoinageAssetState,
)

data class TrackedVoucher(
    val voucher: RecyclerVoucher,
    val state: CoinageAssetState,
)

/**
 * Not on chain yet, but expected to arrive: nothing has proven the transaction minting it never ran.
 *
 * This is what keeps a freshly-split change coin visible instead of vanishing for a whole mortality window,
 * and it is exactly why absence alone cannot be read as "gone". Finality is the strongest case for counting
 * it, not the cue to stop: at that point only the presence the chain reports is behind.
 */
fun TrackedCoin.isMinting(): Boolean =
    state.isFree && !coin.isOnChain && state.minterStatus?.canArrive == true

/** Registered on chain and working its way into a ring: not usable yet, but it exists. */
fun TrackedVoucher.isOnboarding(): Boolean =
    state.isFree && voucher.location is RecyclerVoucher.Location.Onboarding

/**
 * Nowhere on chain yet, but expected to arrive: nothing has proven the transaction minting it never ran.
 *
 * A voucher whose minting transaction failed is not on its way anywhere, and counting it as pending would
 * leave money in the balance that can never arrive. Finality is the opposite case and must still count: the
 * mint is then most certainly done, and only the location the chain reports is behind.
 */
fun TrackedVoucher.isMinting(): Boolean =
    state.isFree && voucher.location is RecyclerVoucher.Location.Unknown && state.minterStatus?.canArrive == true
