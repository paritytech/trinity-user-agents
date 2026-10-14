plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_account_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":bindings:bandersnatch-crypto"))

    api(libs.nova.substrate.sdk)

    implementation(libs.hilt.android)
}