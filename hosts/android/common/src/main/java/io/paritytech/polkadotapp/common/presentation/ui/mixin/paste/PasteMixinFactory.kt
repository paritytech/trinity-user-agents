package io.paritytech.polkadotapp.common.presentation.ui.mixin.paste

import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import javax.inject.Inject

class PasteMixinFactory @Inject constructor(
    private val clipboardService: ClipboardService,
) : PasteMixin.Factory {
    override fun create(paste: (clip: String) -> Unit): PasteMixin {
        return ClipboardPasteMixin(clipboardService, paste)
    }
}
