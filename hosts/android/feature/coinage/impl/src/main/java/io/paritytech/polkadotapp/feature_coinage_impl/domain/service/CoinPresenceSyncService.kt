package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinUpdate
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Hop
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.transferHop
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject

/**
 * Keeps each coin's on-chain presence current.
 *
 * It writes only the age, which is also the presence flag: a coin the chain no longer holds goes back to
 * [Coin.Age.Unknown] rather than being marked spent. Why the coin is gone — consumed, reverted, or never
 * minted — is not this service's call; that is the ledger's, and conflating the two is what the old
 * spent-state column did.
 */
class CoinPresenceSyncService @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val coinRepository: CoinRepository,
) {
    context(scope: ComputationalScope)
    suspend fun start() {
        val asset = chainAssetProvider.asset()

        // TODO: it is wasteful to subscribe to all coins. We should rather subscribe to those that are proven to be terminal
        coinRepository.subscribeAllCoins()
            .filter { it.isNotEmpty() }
            .distinctUntilChanged()
            .flatMapLatest { coins ->
                coinageLogD("presence-subscribe coins=${coins.size} offChain=${coins.count { !it.isOnChain }}")

                coinRepository.subscribeCoinsInfoFor(asset.chainId, coins.map { it.accountId })
                    .map { it.logFailure("Can't fetch info for coins").getOrEmpty() }
                    .onEach { onChainData ->
                        val updates = coins.toPresenceUpdates(onChainData)

                        coinageLogD(
                            "presence-update watching=${coins.size} " +
                                "present=${onChainData.count { it.value != null }} written=${updates.size}"
                        )

                        coinRepository.updateCoins(updates)
                        coinRepository.updateCoinHops(coins.toHopUpdates(onChainData))
                    }
            }
            .launchIn(scope)
    }

    /**
     * Fills in the hops of coins that arrived knowing nothing about themselves — claimed from a peer, or
     * recovered by backup.
     *
     * The chain's age is the only record of how far such a coin has travelled, and it is not readable at
     * claim time, which is why this is the moment the history gets written. Coins that already know their
     * provenance are left alone: a split's change coin inherits a real history from the coin it came from,
     * and rebuilding it from the age would throw that away.
     */
    private fun List<Coin>.toHopUpdates(
        onChainData: Map<AccountId, OnChainCoinInfo?>
    ): Map<CoinageKeyIndex, List<Hop>> {
        return filter { it.provenance.isUnobserved }
            .mapNotNull { coin ->
                val age = onChainData[coin.accountId]?.age ?: return@mapNotNull null

                // An age of zero leaves the coin unobserved, so a later tick tries again rather than
                // freezing an empty history the moment the coin appears.
                if (age <= 0) return@mapNotNull null

                coin.derivationIndex to coin.hopsForAge(age)
            }
            .toMap()
    }

    private fun Coin.hopsForAge(age: Int): List<Hop> {
        // The chain says how many times the coin moved, never what moved alongside it. Only the newest hop
        // has a crowd we recorded — the batch it was claimed in — so every older one stands alone.
        val newestBundleSize = provenance.incomingBundleSize ?: LONE_TRANSFER

        return List(age) { index ->
            transferHop(if (index == age - 1) newestBundleSize else LONE_TRANSFER)
        }
    }

    private fun List<Coin>.toPresenceUpdates(onChainData: Map<AccountId, OnChainCoinInfo?>) = mapNotNull { coin ->
        val coinInfo = onChainData[coin.accountId]

        if (coinInfo != null && coinInfo.value != coin.valueExponent.value) {
            coinageLogE(
                "TrackingCoin: ${coin.accountId} with exponent ${coin.valueExponent} " +
                    "doesn't match on chain exponent ${coinInfo.value}"
            )
        }

        val onChain = coinInfo != null
        val currentAge = (coin.age as? Coin.Age.Known)?.value

        // We do not erase age in case the coin has disappeared from the chain
        val shouldUpdateAge = coinInfo != null && coinInfo.age != currentAge
        val shouldUpdateIsOnChain = onChain != coin.isOnChain

        if (shouldUpdateAge || shouldUpdateIsOnChain) {
            CoinUpdate(accountId = coin.accountId, onChain = onChain, age = coinInfo?.age)
        } else {
            null
        }
    }

    private companion object {
        const val LONE_TRANSFER = 1
    }
}
