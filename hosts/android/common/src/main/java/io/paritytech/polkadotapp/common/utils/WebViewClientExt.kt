package io.paritytech.polkadotapp.common.utils

import android.webkit.WebResourceResponse
import android.webkit.WebViewClient
import java.io.ByteArrayInputStream

fun WebViewClient.notFoundResponse(): WebResourceResponse {
    return WebResourceResponse(
        "text/plain",
        "UTF-8",
        ByteArrayInputStream("Not Found".toByteArray())
    ).apply {
        setStatusCodeAndReasonPhrase(404, "Not Found")
    }
}
