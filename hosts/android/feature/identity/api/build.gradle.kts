plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_identity_api"
}

dependencies {
    api(project(":common"))
    api(project(":feature:people:api"))
    api(project(":feature:transactions:api"))
}
