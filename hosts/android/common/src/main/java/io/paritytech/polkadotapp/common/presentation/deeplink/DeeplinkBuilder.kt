package io.paritytech.polkadotapp.common.presentation.deeplink

import android.net.Uri

object DeeplinkBuilder {
    fun startDeeplinkBuilder(scheme: String): Uri.Builder {
        return Uri.Builder()
            .scheme(scheme)
    }
}

fun Uri.getQueryParameterOrThrow(parameterName: String): String {
    return requireNotNull(getQueryParameter(parameterName))
}
