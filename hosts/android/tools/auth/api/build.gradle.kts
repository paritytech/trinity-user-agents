plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_auth_api"
}

dependencies {
    api(project(":common"))
}