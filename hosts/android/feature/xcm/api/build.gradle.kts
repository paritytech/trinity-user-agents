plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_xcm_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))

    api(libs.nova.substrate.sdk)

    testImplementation(libs.junit)
}