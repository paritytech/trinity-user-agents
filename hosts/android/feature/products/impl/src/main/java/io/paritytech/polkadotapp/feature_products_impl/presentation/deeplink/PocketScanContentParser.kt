package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.toUriResult
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import timber.log.Timber

/**
 * A Pocket link is as likely to arrive as a QR code as it is to be tapped, and the scanner reads a
 * different set than the deeplink router. Without this the same URL is claimed when tapped and
 * reported as an invalid code when scanned.
 */
internal class PocketScanContentParser(
    private val handler: PocketDeepLinkHandler,
) : ScanContentParser {
    override suspend fun canHandle(content: String): Boolean =
        content.toUriResult()
            .map { handler.canHandle(it) }
            .getOrDefault(false)

    context(scope: ComputationalScope)
    override suspend fun handle(content: String): Result<PostParseAction> =
        content.toUriResult()
            .flatMap { handler.handle(it) }
            .flatMap(DeeplinkProcessingOutcome::asScanAction)
}

/**
 * The scanner can navigate or refuse; it has no way to say anything of its own. A refusal the host
 * meant as a message is reported rather than carried as "do nothing", so the user gets the scanner's
 * generic invalid-code dialog, which is also the only thing that resumes decoding. Its distinct
 * reason is logged, since that dialog is the same for all of them.
 */
internal fun DeeplinkProcessingOutcome.asScanAction(): Result<PostParseAction> = when (this) {
    is DeeplinkProcessingOutcome.Navigate -> Result.success(PostParseAction.BackAndThen(navigate))

    is DeeplinkProcessingOutcome.ShowMessage -> {
        Timber.w("pocket: scanned link refused: %s", message)

        Result.failure(PocketScanRefused(message))
    }

    DeeplinkProcessingOutcome.NoOp -> Result.success(PostParseAction.Nothing)
}

internal class PocketScanRefused(message: String) : IllegalArgumentException(message)
