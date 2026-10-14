plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_scan_api"
}

dependencies {
    api(project(":common"))
}
