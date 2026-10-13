plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_chain_resources_impl"
}

dependencies {
    api(project(":feature:chain-resources:api"))

    implementation(project(":feature:account:api"))
    implementation(project(":feature:dotns-gateway:api"))
    implementation(project(":feature:transactions:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)
}