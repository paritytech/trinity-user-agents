import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_auth_impl"
    val localProperties = gradleLocalProperties(rootDir, providers)

    flavorDimensions += "distribution"

    productFlavors {
        create("gp") {
            dimension = "distribution"
            buildConfigString(
                "GOOGLE_OAUTH_ID",
                localProperties.readSecretOrThrow("GOOGLE_OAUTH_ID")
            )
        }
        create("vanilla") {
            dimension = "distribution"
            buildConfigField("String", "GOOGLE_OAUTH_ID", "\"\"")
        }
    }
}

dependencies {
    api(project(":tools:auth:api"))

    implementation(project(":tools:common"))

    "gpImplementation"(libs.google.api.client)
    "gpImplementation"(libs.google.play.services.auth)
    "gpImplementation"(platform(libs.firebase.bom))
    "gpImplementation"(libs.firebase.auth)
}
