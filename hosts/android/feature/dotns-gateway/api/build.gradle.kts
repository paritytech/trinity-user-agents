plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_dotns_gateway_api"
}

dependencies {
    api(project(":chains"))
    api(project(":common"))
}
