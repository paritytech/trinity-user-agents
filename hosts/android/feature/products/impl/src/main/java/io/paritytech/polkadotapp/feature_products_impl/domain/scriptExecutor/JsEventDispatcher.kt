package io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor

import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent

/**
 * Interface for dispatching events from UI back to JS.
 * Used by interactive widgets (Button, TextField) to invoke registered callbacks.
 */
interface JsEventDispatcher {
    /**
     * Dispatch an event to JS.
     * @param actionId The action ID registered by JS
     * @param payload Data to pass to the callback (e.g., new text value for TextField)
     */
    fun dispatchEvent(event: JsUiEvent)
}
