package io.paritytech.polkadotapp.common.presentation

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.browseUrl
import javax.inject.Inject

interface BrowserNavigator {
    fun open(url: String)

    fun open(@StringRes linkResId: Int)
}

class RealBrowserNavigator @Inject constructor(
    private val contextManager: ContextManager,
) : BrowserNavigator {
    override fun open(linkResId: Int) {
        open(contextManager.applicationContext.getString(linkResId))
    }

    override fun open(url: String) {
        contextManager.requireActivity().browseUrl(url)
    }
}
