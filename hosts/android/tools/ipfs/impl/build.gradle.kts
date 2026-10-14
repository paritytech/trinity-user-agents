plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_ipfs_impl"
}

dependencies {
    api(project(":tools:ipfs:api"))

    implementation(project(":common"))
    implementation(project(":tools:remoteconfig:api"))

    implementation(libs.bundles.squareup.okhttp3)
}
