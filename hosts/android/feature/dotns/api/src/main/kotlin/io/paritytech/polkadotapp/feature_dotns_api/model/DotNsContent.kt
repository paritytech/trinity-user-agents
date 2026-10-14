package io.paritytech.polkadotapp.feature_dotns_api.model

import android.net.Uri

data class DotNsContent(
    val dotNsName: String,
    val contentHash: String,
    val localUri: Uri
)
