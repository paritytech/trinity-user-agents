package io.paritytech.polkadotapp.feature_wallet_impl.presentation

import io.mockk.verify
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter

internal const val ONRAMP_URL = "onramp.example"
internal const val OFFRAMP_URL = "offramp.example"

internal fun PocketRouter.verifySheetOpenedOnce(url: String) {
    verify(exactly = 1) { openSpaSheet(url) }
}

internal fun PocketRouter.verifySheetOpenedTwice(url: String) {
    verify(exactly = 2) { openSpaSheet(url) }
}

internal fun PocketRouter.verifySheetNeverOpened(url: String) {
    verify(exactly = 0) { openSpaSheet(url) }
}
