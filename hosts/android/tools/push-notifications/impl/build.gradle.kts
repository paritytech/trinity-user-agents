import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

val localProperties = gradleLocalProperties(rootDir, providers)
val iosBundleId = localProperties.readSecretOrThrow("IOS_BUNDLE_ID")

android {
    namespace = "io.paritytech.polkadotapp.tools_push_notifications_impl"

    buildTypes {
        getByName("debug") {
            buildConfigString("IOS_BUNDLE_ID", "$iosBundleId.develop")
        }
        getByName("nightly") {
            buildConfigString("IOS_BUNDLE_ID", iosBundleId)
        }
        getByName("safetynet") {
            buildConfigString("IOS_BUNDLE_ID", "$iosBundleId.safety")
        }
        getByName("release") {
            buildConfigString("IOS_BUNDLE_ID", iosBundleId)
        }
    }

    flavorDimensions += "distribution"

    productFlavors {
        create("gp") { dimension = "distribution" }
        create("vanilla") { dimension = "distribution" }
    }
}

dependencies {
    api(project(":tools:push-notifications:api"))
    implementation(project(":tools:jwt-auth:api"))

    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
}
