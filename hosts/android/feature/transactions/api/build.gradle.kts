plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_transactions_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":design"))

    api(project(":feature:account:api"))

    implementation(libs.hilt.android)
}