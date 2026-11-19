package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

/**
 * Whether a session acknowledges an incoming request with a response statement. A request left unanswered is still
 * delivered to the session's listeners; only the response statement, and with it the signature, is withheld.
 */
fun interface IncomingRequestResponsePolicy {
    fun shouldRespond(messages: List<EncodedMessage>): Boolean

    companion object {
        val Always = IncomingRequestResponsePolicy { true }
    }
}
