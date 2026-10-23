plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_dotns_api"
}

dependencies {
    api(project(":common"))

    testImplementation(libs.junit)
}
