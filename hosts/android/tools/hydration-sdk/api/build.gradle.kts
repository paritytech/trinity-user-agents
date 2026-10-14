plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_hydration_sdk_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))

    api(project(":feature:transactions:api"))
}