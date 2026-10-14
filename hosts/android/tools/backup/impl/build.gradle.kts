import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
}

android {
    namespace = "io.paritytech.polkadotapp.tools_backup_impl"
    val localProperties = gradleLocalProperties(rootDir, providers)

    defaultConfig {
        buildConfigString(
            "FIRESTORE_DATABASE_ID",
            localProperties.readSecretOrThrow("FIRESTORE_DATABASE_ID")
        )
    }

    buildTypes {
        getByName("release") {
            buildConfigString("BACKUP_FILE_SUFFIX", "production")
            buildConfigString("BACKUP_KEY_SUFFIX", "-production")
        }

        getByName("debug") {
            buildConfigString("BACKUP_FILE_SUFFIX", "debug")
            buildConfigString("BACKUP_KEY_SUFFIX", "-debug")
        }

        getByName("nightly") {
            buildConfigString("BACKUP_FILE_SUFFIX", "nightly")
            buildConfigString("BACKUP_KEY_SUFFIX", "-nightly")
        }

        getByName("safetynet") {
            buildConfigString("BACKUP_FILE_SUFFIX", "safetynet")
            buildConfigString("BACKUP_KEY_SUFFIX", "-safetynet")
        }
    }

    flavorDimensions += "distribution"

    productFlavors {
        create("gp") { dimension = "distribution" }
        create("vanilla") { dimension = "distribution" }
    }
}

dependencies {
    api(project(":tools:backup:api"))

    implementation(project(":chains"))
    implementation(project(":tools:common"))
    implementation(project(":tools:auth:api"))

    implementation(libs.google.play.services.auth)
    implementation(libs.google.api.client)
    implementation(libs.google.drive)
    implementation(libs.bouncycastle.jdk18)
    implementation(libs.androidx.credentials.core)
    implementation(libs.androidx.credentials.play)

    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.firestore)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
