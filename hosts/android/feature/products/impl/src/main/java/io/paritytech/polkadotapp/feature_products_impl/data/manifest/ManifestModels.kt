package io.paritytech.polkadotapp.feature_products_impl.data.manifest

import com.google.gson.annotations.JsonAdapter
import com.google.gson.annotations.SerializedName
import io.paritytech.polkadotapp.feature_products_api.model.ProductIcon
import io.paritytech.polkadotapp.feature_products_api.model.SemVer

// Wire-format DTOs for the product manifest (v1). Everything is nullable and validated by the
// parser — these cross the dotNS trust boundary, and an absent key leaves the field null.

internal class RootManifestRemote(
    @SerializedName("\$v") val version: Int? = null,
    val displayName: String? = null,
    val description: String? = null,
    val icon: IconRemote? = null,
)

internal class IconRemote(
    val cid: String? = null,
    val format: String? = null,
)

// One DTO for all kinds: the expected kind is known from the subname, so the parser reads the
// matching fields and ignores the rest.
internal class ExecutableManifestRemote(
    @SerializedName("\$v") val version: Int? = null,
    val kind: String? = null,
    @JsonAdapter(SemVerArrayAdapter::class)
    val appVersion: SemVer? = null,
    // worker
    val entrypoint: String? = null,
    val includes: IncludesRemote? = null,
    val pocket: PocketRemote? = null,
    // widget
    val description: String? = null,
    val dimensions: DimensionsRemote? = null,
)

internal class IncludesRemote(
    val chat: Boolean? = null,
    val pocket: Boolean? = null,
)

internal class PocketRemote(
    val cards: List<PocketCardRemote>? = null,
)

internal class PocketCardRemote(
    val id: String? = null,
    val title: String? = null,
    val preview: String? = null,
)

internal class DimensionsRemote(
    val height: List<Int>? = null,
    val width: Int? = null,
)

internal class RootManifest(
    val displayName: String,
    val icon: ProductIcon?,
)
