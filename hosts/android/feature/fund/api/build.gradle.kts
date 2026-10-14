plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_fund_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))

    api(project(":feature:prices:api"))
}