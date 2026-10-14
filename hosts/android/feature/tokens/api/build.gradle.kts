plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_tokens_api"
}

dependencies {
    api(libs.hilt.lifecycle.viewmodel.compose)

    api(project(":common"))
    api(project(":design"))
    api(project(":chains"))

    api(project(":feature:balances:api"))
    api(project(":feature:transactions:api"))
}