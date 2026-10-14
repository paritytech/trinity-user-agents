package io.paritytech.polkadotapp.feature_products_api.model

// [manifestKind] is both the manifest `kind` discriminator and the `<kind>.<base>` subname label.
enum class ExecutableKind(val manifestKind: String) {
    APP("app"),
    WIDGET("widget"),
    WORKER("worker"),
}
