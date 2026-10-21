package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.validateValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.validateValueExponents
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.CurrentInstallationProvider
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject

class RealVoucherAllocator @Inject constructor(
    private val voucherRepository: VoucherRepository,
    private val currentInstallationProvider: CurrentInstallationProvider,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val boundsRepository: ExponentBoundsRepository,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : VoucherAllocator {
    private val allocationMutex = Mutex()

    override suspend fun allocate(valueExponent: ValueExponent): Result<RecyclerVoucher> =
        boundsRepository.validateValueExponent(chainAssetProvider.chainId(), valueExponent).flatMap { validExponent ->
            withBackedUpInstallation { installation ->
                val derivationIndex = CoinageKeyIndex(installation, voucherRepository.getNextDerivationIndex(installation))
                val voucher = createVoucherForIndex(derivationIndex, validExponent)
                voucher.apply { voucherRepository.saveNew(this) }
            }
        }

    override suspend fun allocateAll(valueExponents: List<ValueExponent>): Result<List<RecyclerVoucher>> =
        boundsRepository.validateValueExponents(chainAssetProvider.chainId(), valueExponents).flatMap { validExponents ->
            withBackedUpInstallation { installation ->
                val nextDerivationIndex = voucherRepository.getNextDerivationIndex(installation)

                val vouchers = validExponents.mapIndexed { index, value ->
                    createVoucherForIndex(CoinageKeyIndex(installation, nextDerivationIndex + index), value)
                }

                voucherRepository.saveNew(vouchers)

                vouchers
            }
        }

    private suspend fun <T> withBackedUpInstallation(allocate: suspend (CoinageInstallationId) -> T): Result<T> =
        currentInstallationProvider.awaitBackedUp().flatMap { installation ->
            allocationMutex.withLock { runCancellableCatching { allocate(installation) } }
        }

    private suspend fun createVoucherForIndex(derivationIndex: CoinageKeyIndex, valueExponent: ValueExponent): RecyclerVoucher {
        val publicKey = voucherRingDerivation.memberKeyOf(derivationIndex)

        return RecyclerVoucher(
            ringVrfKeyIndex = derivationIndex,
            ringVrfPublicKey = publicKey,
            location = RecyclerVoucher.Location.Unknown,
            recyclerValue = valueExponent,
            // A voucher outside a ring hides in nothing, so zero is the truth rather than a stand-in. The
            // max is a different matter: it needs a ring index the chain has not assigned yet, so it stays
            // unfrozen until the location service sees the voucher land.
            recyclerFungibility = RecyclerFungibility.NONE,
            maxRecyclerFungibility = null,
        )
    }
}
