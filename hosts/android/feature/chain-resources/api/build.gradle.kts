plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_chain_resources_api"
}

dependencies {
    api(project(":chains"))
    api(project(":common"))
}