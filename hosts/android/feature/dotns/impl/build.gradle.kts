plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_dotns_impl"
}

dependencies {
    api(project(":feature:dotns:api"))

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":chains"))
    implementation(project(":tools:car-parser"))
    implementation(project(":tools:ipfs:api"))
    implementation(project(":tools:remoteconfig:api"))
    implementation(project(":feature:revive:api"))

    implementation(libs.squareup.okhttp3.core)
    implementation(libs.web3j.abi)
    implementation(libs.bouncycastle.jdk18)

    testImplementation(project(":test-shared"))
}
