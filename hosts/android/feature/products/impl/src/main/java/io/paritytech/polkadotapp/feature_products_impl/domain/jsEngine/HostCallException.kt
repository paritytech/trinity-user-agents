package io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine

/**
 * Thrown (or returned as a failure) by a host-call handler to surface a typed error code to the product.
 */
class HostCallException(val code: String, message: String) : Exception(message)
