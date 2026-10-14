package io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine

/**
 * Thrown (or returned as a failure) by a host-call handler to surface a typed error code to JS.
 * The bridge serializes it as `{"error":{"code":<code>,"message":<message>}}`.
 */
class HostCallException(val code: String, message: String) : Exception(message)
