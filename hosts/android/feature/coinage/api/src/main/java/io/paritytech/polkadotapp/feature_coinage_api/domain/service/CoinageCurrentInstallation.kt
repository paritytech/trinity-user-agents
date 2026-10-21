package io.paritytech.polkadotapp.feature_coinage_api.domain.service

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId

/** The installation this app allocates keys under. */
interface CoinageCurrentInstallation {
    /** The current installation, created on first use. It may not be backed up yet. */
    suspend fun getOrCreateCurrent(): CoinageInstallationId
}
