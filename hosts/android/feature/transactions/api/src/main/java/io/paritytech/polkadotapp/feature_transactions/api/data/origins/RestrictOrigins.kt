package io.paritytech.polkadotapp.feature_transactions.api.data.origins

import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.FixedValueTransactionExtension

/**
 * Must be enabled when the origin is a restricted entity that wants a free tx to happen.
 * Examples: PersonalIdentity, PersonalAlias, ReferredCandidate, LitePerson
 *
 * Full list: https://github.com/paritytech/individuality-community/blob/fce93ef38a15c673a8b0b208362bc46ae755c7d7/runtimes/next-people-paseo/src/lib.rs#L820
 *
 * For other cases it does not really matter whether this is enabled or not. The only difference is that disabled extension produce lower pre-dispatch weights
 */
class RestrictOrigins(enabled: Boolean) : FixedValueTransactionExtension(
    name = "RestrictOrigins",
    implicit = null,
    explicit = enabled
)
