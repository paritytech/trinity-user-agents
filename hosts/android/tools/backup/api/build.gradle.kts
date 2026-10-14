plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_backup_api"
}

dependencies {
    api(project(":common"))
}