package io.paritytech.polkadotapp.feature_coinage_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset

/**
 * Collects what one coinage extrinsic consumes and mints, allocating fresh assets as it goes.
 *
 * A call site describes the transaction — consume this coin, mint this change, hand these to the recipient —
 * instead of assembling two parallel lists by hand and keeping them in step. It writes no status: the ledger
 * records the transaction, and until it does nothing here has been committed to.
 */
interface CoinageTransaction {
    interface Factory {
        fun newTransaction(): CoinageTransaction
    }

    suspend fun mintCoins(valueExponents: List<ValueExponent>, provenance: CoinProvenance): Result<List<Coin>>

    suspend fun mintVoucher(valueExponent: ValueExponent): Result<RecyclerVoucher>

    fun consumeCoins(coins: List<Coin>)

    /** A coin whose key a peer sent us: an input, never one of our assets. */
    fun consumeReceivedCoin(publicKey: DataByteArray)

    fun useVouchers(vouchers: List<RecyclerVoucher>)

    /** Records that these assets' keys are leaving the device. */
    fun handOff(assets: List<OwnAsset>)

    fun build(): CoinageTransactionAssets
}

data class CoinageTransactionAssets(
    val inputs: List<CoinageInput>,
    val outputs: List<OwnAsset>,
    val handedOff: List<OwnAsset>,
)

suspend fun CoinageTransaction.mintCoin(valueExponent: ValueExponent, provenance: CoinProvenance): Result<Coin> =
    mintCoins(listOf(valueExponent), provenance).map { it.single() }

fun CoinageTransaction.consumeCoin(coin: Coin) = consumeCoins(listOf(coin))

fun CoinageTransaction.handOffCoins(coins: List<Coin>) = handOff(coins.map { OwnAsset.Coin(it.derivationIndex) })

/** Mint output coins and immediately hand them to the recipient. Recipient coins always do both. */
suspend fun CoinageTransaction.mintAndHandOffCoins(
    valueExponents: List<ValueExponent>,
    provenance: CoinProvenance
): Result<List<Coin>> = mintCoins(valueExponents, provenance).onSuccess { handOffCoins(it) }
