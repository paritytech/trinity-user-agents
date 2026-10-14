plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_biometrics_api"
}

dependencies {
    api(project(":common"))
}