package io.paritytech.polkadotapp.feature_coinage_api.domain.service

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId

/** The installation this app allocates keys under. */
interface CurrentInstallationProvider {
    /** The current installation, once its on-chain registration has completed. */
    suspend fun awaitBackedUp(): Result<CoinageInstallationId>
}
