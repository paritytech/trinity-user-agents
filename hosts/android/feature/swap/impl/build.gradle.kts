plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_swap_impl"
}

dependencies {
    api(project(":feature:swap:api"))

    ksp(libs.hilt.android.compiler)

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":common"))
    implementation(project(":chains"))

    implementation(project(":feature:prices:api"))
    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:balances:api"))
    implementation(project(":feature:transfers:api"))
    implementation(project(":feature:cross-chain-transfers:api"))

    implementation(project(":tools:hydration-sdk:api"))
    implementation(project(":tools:assethub-sdk:api"))

    implementation(libs.nova.substrate.sdk)

}