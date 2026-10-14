package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import kotlinx.serialization.Serializable

/**
 * Legacy variant of [SsoSigningRawPayloadScale] where the account is a plain 32-byte account id
 * rather than a product account tuple. Mirrors `SignRawLegacyRequest` on the JS host-papp side.
 */
@Serializable
class SsoSignRawLegacyRequestScale(
    val account: AccountIdScale,
    val type: SsoPayloadTypeScale,
)
