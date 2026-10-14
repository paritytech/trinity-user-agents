plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    alias(libs.plugins.kotlin.parcelize)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_people_impl"

    buildFeatures {
        viewBinding = true
    }
}

dependencies {
    api(project(":feature:people:api"))

    implementation(project(":tools:jwt-auth:api"))
    implementation(project(":feature:chats:api"))

    implementation(libs.hilt.android)
    implementation(libs.hilt.androidx.work)
    ksp(libs.hilt.android.compiler)
    ksp(libs.hilt.androidx.compiler)

    implementation(project(":design"))

    implementation(libs.bundles.androidx.camera)

    implementation(libs.coil.kt)

    implementation(libs.androidx.work.runtime)
}
