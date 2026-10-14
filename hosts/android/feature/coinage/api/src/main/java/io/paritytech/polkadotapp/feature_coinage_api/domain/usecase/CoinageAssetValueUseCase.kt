package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset

/**
 * What assets of ours are worth.
 *
 * One entry point so that every surface reporting how much of an operation actually landed — a chat payment,
 * a send screen, an external payment — reaches the same number from the same rows, rather than each joining
 * local assets to the balance conversion in its own way.
 */
interface CoinageAssetValueUseCase {
    /** An asset we do not hold contributes nothing: a caller sums what it can account for, not what it hoped for. */
    suspend fun valueOf(assets: List<OwnAsset>): Result<Balance>
}
