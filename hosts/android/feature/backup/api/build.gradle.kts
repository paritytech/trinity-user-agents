plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.parcelize)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_backup_api"
}

dependencies {
    api(project(":common"))

    api(project(":tools:backup:api"))
}
