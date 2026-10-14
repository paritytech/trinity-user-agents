package io.paritytech.polkadotapp.feature_products_api.presentation.spaHost

import android.view.ViewGroup
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Renders a WebView owned elsewhere (the runtime holds its lifecycle — this only displays it).
 *
 * The factory runs once, so the WebView is swapped in `update` and detached from its previous
 * parent first — otherwise switching the hosted product would leave the wrong one on screen.
 */
@Composable
fun ProductWebViewHost(modifier: Modifier = Modifier, webView: WebView?) {
    AndroidView(
        modifier = modifier,
        factory = { context -> FrameLayout(context) },
        update = { host ->
            if (host.getChildAt(0) !== webView) {
                host.removeAllViews()
                webView?.let {
                    (it.parent as? ViewGroup)?.removeView(it)
                    host.addView(it)
                }
            }
        },
    )
}
