package io.paritytech.polkadotapp.common.presentation.ui.mixin.paste

import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

interface PasteMixin {
    interface Factory {
        fun create(paste: (clip: String) -> Unit): PasteMixin
    }

    val pasteVisibility: Flow<Boolean>

    fun onPasteClicked()
}

internal class ClipboardPasteMixin(
    private val clipboardService: ClipboardService,
    private val applyPaste: (clip: String) -> Unit,
) : PasteMixin {
    override val pasteVisibility: Flow<Boolean>
        get() = clipboardService.observePrimaryClip().map { it.isNullOrBlank().not() }

    override fun onPasteClicked() {
        clipboardService.getPrimaryClip()?.let(applyPaste)
    }
}
