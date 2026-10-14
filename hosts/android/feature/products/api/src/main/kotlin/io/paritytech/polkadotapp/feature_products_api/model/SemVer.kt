package io.paritytech.polkadotapp.feature_products_api.model

/** `major.minor.patch` with RFC-0001's optional build identifier. */
@ConsistentCopyVisibility
data class SemVer private constructor(
    val major: Int,
    val minor: Int,
    val patch: Int,
    val build: String?,
) {
    override fun toString(): String {
        val version = "$major.$minor.$patch"

        // Build metadata is joined with `+`: `-` would mean a prerelease, which RFC-0001 does not use.
        return if (build != null) "$version+$build" else version
    }

    companion object {
        /** Legacy products publish no manifest, so they have no version to report. */
        val ZERO = SemVer(major = 0, minor = 0, patch = 0, build = null)

        fun fromComponents(major: Int, minor: Int, patch: Int, build: String?): SemVer =
            SemVer(major = major, minor = minor, patch = patch, build = build?.ifEmpty { null })
    }
}
