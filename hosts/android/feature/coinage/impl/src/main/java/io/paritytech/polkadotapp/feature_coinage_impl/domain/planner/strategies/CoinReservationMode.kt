package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset

enum class CoinReservationMode {
    PREPARE,
    PRECOMMIT,
}

suspend fun CoinageTransactionService.reserveAssets(
    assets: List<OwnAsset>,
    mode: CoinReservationMode,
): Result<CoinageHandoffCommit> = when (mode) {
    CoinReservationMode.PREPARE -> prepareHandoff(assets)
    CoinReservationMode.PRECOMMIT -> preCommitHandoff(assets)
}
