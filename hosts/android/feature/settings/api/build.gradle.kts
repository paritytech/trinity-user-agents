plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_settings_api"
}

dependencies {
    api(project(":common"))
}