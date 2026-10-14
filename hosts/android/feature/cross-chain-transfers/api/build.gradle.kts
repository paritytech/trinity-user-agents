plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_cross_chain_transfers_api"
}

dependencies {
    api(project(":common"))
    api(project(":chains"))
    api(project(":feature:xcm:api"))
    api(project(":feature:transactions:api"))
}