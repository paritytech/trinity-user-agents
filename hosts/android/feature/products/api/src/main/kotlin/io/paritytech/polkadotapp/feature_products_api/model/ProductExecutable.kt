package io.paritytech.polkadotapp.feature_products_api.model

/** App and Widget are served at [host]; Worker is located by its full [Worker.scriptUrl]. */
sealed interface ProductExecutable {
    val appVersion: SemVer

    data class App(
        val host: ExecutableHost,
        override val appVersion: SemVer,
    ) : ProductExecutable

    data class Widget(
        val host: ExecutableHost,
        override val appVersion: SemVer,
        val description: String?,
        val heights: List<Int>,
        val width: Int,
    ) : ProductExecutable

    data class Worker(
        val scriptUrl: String,
        override val appVersion: SemVer,
        val includesChat: Boolean,
        val includesPocket: Boolean,
        val pocketCards: List<PocketCardDefinition>,
    ) : ProductExecutable
}
