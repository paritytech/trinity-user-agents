package io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive

// Escapes a value crossing the dotNS trust boundary for embedding in script source; a JSON string
// is a valid JS string. The result INCLUDES its quotes — embed it bare, not inside more quotes.
internal fun String.toJsStringLiteral(): String =
    Json.encodeToString(JsonPrimitive.serializer(), JsonPrimitive(this))
