package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.compositionLocalOf
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent

typealias JsUiEventHandler = (actionId: String, eventType: JsUiEvent.Type) -> Unit

val LocalJsEventHandler = compositionLocalOf<JsUiEventHandler> { { _, _ -> } }
