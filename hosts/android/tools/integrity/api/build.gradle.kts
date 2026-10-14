plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_integrity_api"
}

dependencies {
    api(project(":common"))
}