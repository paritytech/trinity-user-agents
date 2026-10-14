package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface DotNsGatewayOrigins {
    /**
     * Origin for `register_name(who, label, link)`: a general (unsigned) transaction
     * authenticated by the `AsDotnsGateway` extension with a People ring proof and
     * an sr25519 consent signature from [who].
     */
    suspend fun asPersonRegistration(who: AccountId, label: String, link: DotNsLink): TransactionOrigin
}
