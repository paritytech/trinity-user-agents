package io.paritytech.polkadotapp.feature_balances_api.data.type.external

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.UntypedOrmlCurrencyId

sealed class ExternalAssetId {
    companion object;

    data object Native : ExternalAssetId()

    class Orml(val currencyId: UntypedOrmlCurrencyId) : ExternalAssetId()

    class HydrationEvm(val assetId: UntypedOrmlCurrencyId) : ExternalAssetId()
}
