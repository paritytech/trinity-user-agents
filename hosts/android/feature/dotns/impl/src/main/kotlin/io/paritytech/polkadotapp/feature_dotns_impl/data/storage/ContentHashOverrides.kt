package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

interface ContentHashOverrides {
    fun getContentHashOverride(dotNsName: String): ContentHash?

    fun putContentHashOverride(dotNsName: String, contentHash: ContentHash)
}
