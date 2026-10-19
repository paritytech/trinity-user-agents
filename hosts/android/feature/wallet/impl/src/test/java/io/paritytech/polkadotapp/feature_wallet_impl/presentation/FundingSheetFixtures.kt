package io.paritytech.polkadotapp.feature_wallet_impl.presentation

import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify

internal const val ONRAMP_URL = "onramp.example"
internal const val OFFRAMP_URL = "offramp.example"

internal fun PocketRouter.verifySheetOpenedOnce(url: String) {
    verify(this).openSpaSheet(url)
}

internal fun PocketRouter.verifySheetOpenedTwice(url: String) {
    verify(this, times(2)).openSpaSheet(url)
}

internal fun PocketRouter.verifySheetNeverOpened(url: String) {
    verify(this, never()).openSpaSheet(url)
}
