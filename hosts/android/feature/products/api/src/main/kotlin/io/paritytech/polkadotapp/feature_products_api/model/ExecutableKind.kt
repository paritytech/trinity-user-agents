package io.paritytech.polkadotapp.feature_products_api.model

import uniffi.truapi.ProductExecutionKind

// [manifestKind] is both the manifest `kind` discriminator and the `<kind>.<base>` subname label.
enum class ExecutableKind(val manifestKind: String, val executionKind: ProductExecutionKind) {
    APP("app", ProductExecutionKind.APP),
    WIDGET("widget", ProductExecutionKind.WIDGET),
    WORKER("worker", ProductExecutionKind.WORKER),
}
