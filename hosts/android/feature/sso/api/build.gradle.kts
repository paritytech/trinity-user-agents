plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_sso_api"
}

dependencies {
    api(project(":common"))

    implementation(libs.hilt.android)
}
