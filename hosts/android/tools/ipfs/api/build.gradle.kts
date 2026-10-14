plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_ipfs_api"
}

dependencies {
    // Exposes multihash/multibase types referenced by the public IPFS API (Cid, Multihash).
    // Intentionally does NOT depend on :common — :common contributes IPFS components to its
    // ImageLoader via a seam it owns, so adding a dependency here would create a cycle.
    api(libs.bundles.multiformats)

    testImplementation(libs.junit)
}
