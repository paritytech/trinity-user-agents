package io.paritytech.polkadotapp.feature_products_api.model

/**
 * The dotNS host one executable is served from — `app.coinflip.dot` for the app of `coinflip.dot`.
 * Distinct from [ProductId] on purpose: this addresses content, never identity.
 */
@JvmInline
value class ExecutableHost(val value: String) {
    override fun toString(): String = value
}
