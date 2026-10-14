plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_swap_api"
}

dependencies {
    api(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    api(project(":feature:account:api"))
    api(project(":feature:transactions:api"))
}