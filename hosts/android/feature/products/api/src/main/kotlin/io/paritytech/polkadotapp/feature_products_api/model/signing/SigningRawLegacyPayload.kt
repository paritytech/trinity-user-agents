package io.paritytech.polkadotapp.feature_products_api.model.signing

import io.paritytech.polkadotapp.common.domain.model.AccountId

class SigningRawLegacyPayload(
    val account: AccountId,
    val type: RawPayloadContent,
)
