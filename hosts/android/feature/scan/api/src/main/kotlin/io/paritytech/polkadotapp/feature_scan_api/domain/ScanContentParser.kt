package io.paritytech.polkadotapp.feature_scan_api.domain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope

interface ScanContentParser {
    suspend fun canHandle(content: String): Boolean

    context(scope: ComputationalScope)
    suspend fun handle(content: String): Result<PostParseAction>
}

sealed interface PostParseAction {
    // This can be used to wrap external behaviors that do navigation without intervening with them directly
    class BackAndThen(val postBackNavigation: () -> Unit) : PostParseAction

    object Nothing : PostParseAction
}
