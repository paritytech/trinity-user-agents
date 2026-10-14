plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_prices_api"
}

dependencies {
    api(libs.hilt.lifecycle.viewmodel.compose)

    api(project(":common"))
    api(project(":chains"))
    api(project(":design"))
}