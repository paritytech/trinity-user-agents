import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_integrity_impl"
    val localProperties = gradleLocalProperties(rootDir, providers)

    flavorDimensions += "distribution"

    productFlavors {
        create("gp") {
            dimension = "distribution"
            buildConfigField(
                "long",
                "GOOGLE_PROJECT_ID",
                localProperties.readSecretOrThrow("GOOGLE_PROJECT_ID")
            )
        }
        create("vanilla") {
            dimension = "distribution"
            buildConfigField("long", "GOOGLE_PROJECT_ID", "0L")
        }
    }
}

dependencies {
    api(project(":tools:integrity:api"))

    implementation(libs.google.integrity)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
