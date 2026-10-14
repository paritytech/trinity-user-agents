plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.tools_media_connection_api"
}

dependencies {
    implementation(project(":common"))
    implementation(project(":design"))

    implementation(libs.nova.substrate.serialization)
}
