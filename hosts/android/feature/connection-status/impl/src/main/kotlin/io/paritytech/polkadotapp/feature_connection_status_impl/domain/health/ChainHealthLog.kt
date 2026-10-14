package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import timber.log.Timber

private const val CHAIN_HEALTH_LOG_TAG = "ChainHealth"

internal val chainHealthLog: Timber.Tree
    get() = Timber.tag(CHAIN_HEALTH_LOG_TAG)
