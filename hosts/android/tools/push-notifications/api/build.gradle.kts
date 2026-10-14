plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_push_notifications_api"
}

dependencies {
    api(project(":common"))
}