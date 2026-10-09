plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.chains"
}

dependencies {
    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    implementation(project(":common"))
    implementation(project(":database"))
    implementation(project(":tools:remoteconfig:api"))

    implementation(libs.androidx.appcompat)

    api(libs.google.gson)

    implementation(libs.bouncycastle.jdk18)

    implementation(libs.bundles.squareup.okhttp3)
    api(libs.squareup.retrofit2.core)
    implementation(libs.bundles.squareup.retrofit2.converters)

    implementation(libs.google.android.material)

    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)
}
