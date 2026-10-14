package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.getDerivedMemberKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.validateValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.validateValueExponents
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject

class RealVoucherAllocator @Inject constructor(
    private val voucherRepository: VoucherRepository,
    private val installationRepository: CoinageInstallationRepository,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val boundsRepository: ExponentBoundsRepository,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : VoucherAllocator {
    private val allocationMutex = Mutex()

    override suspend fun allocate(valueExponent: ValueExponent): Result<RecyclerVoucher> =
        allocationMutex.withLock {
            boundsRepository.validateValueExponent(chainAssetProvider.chainId(), valueExponent)
                .mapCatching { validExponent ->
                    val installation = installationRepository.getOrCreateCurrent()
                    val derivationIndex = CoinageKeyIndex(installation, voucherRepository.getNextDerivationIndex(installation))
                    val voucher = createVoucherForIndex(derivationIndex, validExponent)
                    voucher.apply { voucherRepository.saveNew(this) }
                }
        }

    override suspend fun allocateAll(valueExponents: List<ValueExponent>): Result<List<RecyclerVoucher>> = allocationMutex.withLock {
        boundsRepository.validateValueExponents(chainAssetProvider.chainId(), valueExponents)
            .mapCatching { validExponents ->
                val installation = installationRepository.getOrCreateCurrent()
                val nextDerivationIndex = voucherRepository.getNextDerivationIndex(installation)

                val vouchers = validExponents.mapIndexed { index, value ->
                    createVoucherForIndex(CoinageKeyIndex(installation, nextDerivationIndex + index), value)
                }

                voucherRepository.saveNew(vouchers)

                vouchers
            }
    }

    private suspend fun createVoucherForIndex(derivationIndex: CoinageKeyIndex, valueExponent: ValueExponent): RecyclerVoucher {
        val publicKey = voucherRingDerivation.getDerivedMemberKey(derivationIndex)

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
