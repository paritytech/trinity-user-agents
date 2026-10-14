plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_dotns_gateway_impl"
}

dependencies {
    api(project(":feature:dotns-gateway:api"))

    implementation(project(":bindings:bandersnatch-crypto"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:members:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:revive:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":tools:remoteconfig:api"))

    implementation(libs.web3j.abi)
    implementation(libs.bouncycastle.jdk18)

    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockito.core)
    testImplementation(project(":test-shared"))
}
