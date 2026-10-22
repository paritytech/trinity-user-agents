package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.getDerivedAccountId
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.validateValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.validateValueExponents
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.CurrentInstallationProvider
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject

class RealCoinAllocator @Inject constructor(
    private val coinRepository: CoinRepository,
    private val currentInstallationProvider: CurrentInstallationProvider,
    private val keypairDerivation: CoinKeypairDerivation,
    private val boundsRepository: ExponentBoundsRepository,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : CoinAllocator {
    private val allocationMutex = Mutex()

    override suspend fun allocate(valueExponent: ValueExponent, provenance: CoinProvenance): Result<Coin> =
        boundsRepository.validateValueExponent(chainAssetProvider.chainId(), valueExponent).flatMap { validExponent ->
            withBackedUpInstallation { installation ->
                val derivationIndex = CoinageKeyIndex(installation, coinRepository.getNextDerivationIndex(installation))
                val coin = createCoin(derivationIndex, validExponent, provenance)
                coin.apply { coinRepository.saveNew(this) }
            }
        }

    override suspend fun allocateAll(
        valueExponents: List<ValueExponent>,
        provenance: CoinProvenance
    ): Result<List<Coin>> = boundsRepository.validateValueExponents(chainAssetProvider.chainId(), valueExponents).flatMap { validExponents ->
        withBackedUpInstallation { installation ->
            val nextDerivationIndex = coinRepository.getNextDerivationIndex(installation)

            val coins = validExponents.mapIndexed { index, value ->
                createCoin(
                    derivationIndex = CoinageKeyIndex(installation, nextDerivationIndex + index),
                    valueExponent = value,
                    provenance = provenance
                )
            }

            coinRepository.saveNew(coins)

            coins
        }
    }

    private suspend fun <T> withBackedUpInstallation(allocate: suspend (CoinageInstallationId) -> T): Result<T> =
        currentInstallationProvider.awaitBackedUp().flatMap { installation ->
            allocationMutex.withLock { runCancellableCatching { allocate(installation) } }
        }

    private suspend fun createCoin(
        derivationIndex: CoinageKeyIndex,
        valueExponent: ValueExponent,
        provenance: CoinProvenance
    ): Coin = Coin(
        derivationIndex = derivationIndex,
        valueExponent = valueExponent,
        // Freshly allocated: nothing has minted it yet, so the chain has never held it.
        age = Coin.Age.Unknown,
        isOnChain = false,
        accountId = keypairDerivation.getDerivedAccountId(derivationIndex),
        provenance = provenance
    )
}
