plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_common"
}

dependencies {
    api(project(":common"))

    api(libs.google.tasks)
}
