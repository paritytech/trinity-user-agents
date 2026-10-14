plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_balances_api"
}

dependencies {
    api(libs.hilt.lifecycle.viewmodel.compose)

    api(project(":common"))
    api(project(":design"))
    api(project(":chains"))
    api(project(":feature:account:api"))
}