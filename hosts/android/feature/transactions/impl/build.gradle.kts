import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.library")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_transactions_impl"

    defaultConfig {
        val localProperties = gradleLocalProperties(rootDir, providers)
        buildConfigField(
            "String",
            "NIGHTLY_FUNDING_MNEMONIC",
            "\"${localProperties.readSecretOrThrow("NIGHTLY_FUNDING_MNEMONIC")}\""
        )
    }
}

dependencies {
    api(project(":feature:transactions:api"))

    implementation(libs.hilt.android)
    implementation(libs.hilt.androidx.work)
    ksp(libs.hilt.android.compiler)
    ksp(libs.hilt.androidx.compiler)

    implementation(libs.androidx.work.runtime)

    implementation(project(":database"))
    implementation(project(":tools:remoteconfig:api"))

    implementation(libs.nova.substrate.sdk)

    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)
    testImplementation(libs.kotlinx.coroutines.test)
}
