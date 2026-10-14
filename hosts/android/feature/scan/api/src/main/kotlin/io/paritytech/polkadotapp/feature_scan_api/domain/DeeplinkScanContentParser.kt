package io.paritytech.polkadotapp.feature_scan_api.domain

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.toUriResult

class DeeplinkScanContentParser(
    private val deepLinkHandler: DeepLinkHandler,
) : ScanContentParser {
    override suspend fun canHandle(content: String): Boolean {
        return content.toUriResult()
            .map { deepLinkHandler.canHandle(it) }
            .getOrDefault(false)
    }

    context(scope: ComputationalScope)
    override suspend fun handle(content: String): Result<PostParseAction> {
        return content.toUriResult()
            .flatMap { deepLinkHandler.handle(it) }
            .map { it.toPostParseAction() }
    }

    private fun DeeplinkProcessingOutcome.toPostParseAction(): PostParseAction = when (this) {
        is DeeplinkProcessingOutcome.Navigate -> PostParseAction.BackAndThen(navigate)

        is DeeplinkProcessingOutcome.ShowMessage,
        DeeplinkProcessingOutcome.NoOp -> PostParseAction.Nothing
    }
}
