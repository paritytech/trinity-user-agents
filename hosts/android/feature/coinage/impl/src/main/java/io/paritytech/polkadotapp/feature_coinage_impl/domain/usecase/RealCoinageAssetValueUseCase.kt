package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.formatExponentsToBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import javax.inject.Inject

/**
 * Reads the asset rows directly rather than through `CoinageAssetsUseCase`: the only thing a value needs is
 * an exponent, and joining every row against the ledger to find a handful of them scales with the wallet
 * rather than with the question.
 */
class RealCoinageAssetValueUseCase @Inject constructor(
    private val coinRepository: CoinRepository,
    private val voucherRepository: VoucherRepository,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase,
) : CoinageAssetValueUseCase {
    override suspend fun valueOf(assets: List<OwnAsset>): Result<Balance> {
        if (assets.isEmpty()) return Result.success(Balance.ZERO)

        val exponents = exponentsOf(assets)

        return coinageBalanceConverterUseCase.create().map { it.formatExponentsToBalance(exponents) }
    }

    private suspend fun exponentsOf(assets: List<OwnAsset>): List<ValueExponent> {
        val coinIndices = assets.filterIsInstance<OwnAsset.Coin>().map { it.derivationIndex }
        val voucherIndices = assets.filterIsInstance<OwnAsset.Voucher>().map { it.ringVrfIndex }

        val coins = coinRepository.getCoinsBy(coinIndices).map { it.valueExponent }
        val vouchers = voucherRepository.getByRingVrfKeyIndices(voucherIndices).map { it.recyclerValue }

        return coins + vouchers
    }
}
