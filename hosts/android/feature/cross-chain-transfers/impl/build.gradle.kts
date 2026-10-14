plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_cross_chain_transfers_impl"
}

dependencies {
    api(project(":feature:cross-chain-transfers:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":tools:remoteconfig:api"))
    implementation(project(":feature:balances:api"))
}