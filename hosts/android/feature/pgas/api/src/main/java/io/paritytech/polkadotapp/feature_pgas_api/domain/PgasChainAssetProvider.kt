package io.paritytech.polkadotapp.feature_pgas_api.domain

import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider

/**
 * PGAS asset on Asset Hub. Sponsoring decisions read its balance to know whether the
 * caller still has enough headroom to pay for a Revive call.
 */
interface PgasChainAssetProvider : ChainAssetProvider
