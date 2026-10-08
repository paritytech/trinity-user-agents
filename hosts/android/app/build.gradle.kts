import com.android.build.gradle.internal.cxx.configure.gradleLocalProperties

plugins {
    id("polkadotapp.android.application")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.firebase.crashlytics)
    alias(libs.plugins.google.services)
    alias(libs.plugins.sentry.android.gradle)
}

val localProperties = gradleLocalProperties(rootDir, providers)

android {
    namespace = "io.paritytech.polkadotapp.app"

    defaultConfig {
        applicationId = localProperties.readSecretOrThrow("APPLICATION_ID")

        versionCode = computeVersionCode()
        versionName = computeVersionName()

        testInstrumentationRunner = "io.paritytech.polkadotapp.app.HiltTestRunner"

        manifestPlaceholders["appName"] = localProperties.readSecretOrThrow("APPLICATION_NAME")
        manifestPlaceholders["sentryDsn"] = localProperties.readSecretOrNull("SENTRY_DSN") ?: ""

        buildConfigString(
            "CONTACT_EMAIL",
            localProperties.readSecretOrThrow("CONTACT_EMAIL")
        )
        buildConfigString(
            "LOG_COLLECTION_EMAIL",
            localProperties.readSecretOrThrow("LOG_COLLECTION_EMAIL")
        )
        buildConfigString(
            "PRIVACY_POLICY_URL",
            localProperties.readSecretOrThrow("PRIVACY_POLICY_URL")
        )
        buildConfigString(
            "TERMS_OF_USE_URL",
            localProperties.readSecretOrThrow("TERMS_OF_USE_URL")
        )
    }

    signingConfigs {
        create("dev") {
            storeFile = file(localProperties.readSecretOrNull("DEV_KEYSTORE_FILE") ?: "../develop_key.jks")
            keyPassword = localProperties.readSecretOrDefault("CI_KEYSTORE_KEY_PASS", "")
            keyAlias = localProperties.readSecretOrDefault("CI_KEYSTORE_KEY_ALIAS", "")
            storePassword = localProperties.readSecretOrDefault("CI_KEYSTORE_PASS", "")
        }

        create("release") {
            storeFile = file(localProperties.readSecretOrNull("RELEASE_KEYSTORE_FILE") ?: "../release_key.jks")
            keyPassword = localProperties.readSecretOrDefault("RELEASE_KEYSTORE_KEY_PASS", "")
            keyAlias = localProperties.readSecretOrDefault("RELEASE_KEYSTORE_KEY_ALIAS", "")
            storePassword = localProperties.readSecretOrDefault("RELEASE_KEYSTORE_PASS", "")
        }
    }

    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")

            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
        getByName("debug") {
            signingConfig = signingConfigs.getByName("dev")
            applicationIdSuffix = ".debug"
            // A preview build has to be able to name the commit it came from,
            // or a reviewer cannot tell it apart from a cached artifact. Scoped
            // to this build type so no shipped variant can carry it, whatever
            // is set in the environment.
            versionNameSuffix = System.getenv("PREVIEW_COMMIT")
                ?.takeIf { Regex("^[0-9a-f]{40}$").matches(it) }
                ?.let { "+$it" }
            manifestPlaceholders["appName"] = localProperties.readSecretOrNull("DEBUG_APPLICATION_NAME")
                ?: "[Debug] ${localProperties.readSecretOrThrow("APPLICATION_NAME")}"

            buildConfigField("String", "BuildType", "\"debug\"")
        }
        getByName("nightly") {
            matchingFallbacks.add("debug")

            signingConfig = signingConfigs.getByName("dev")
            applicationIdSuffix = ".nightly"
            manifestPlaceholders["appName"] = localProperties.readSecretOrNull("NIGHTLY_APPLICATION_NAME")
                ?: localProperties.readSecretOrThrow("APPLICATION_NAME")
        }
        getByName("safetynet") {
            matchingFallbacks.addAll(listOf("nightly", "debug"))

            signingConfig = signingConfigs.getByName("dev")
            applicationIdSuffix = ".safetynet"
            manifestPlaceholders["appName"] = localProperties.readSecretOrNull("SAFETYNET_APPLICATION_NAME")
                ?: "[Safetynet] ${localProperties.readSecretOrThrow("APPLICATION_NAME")}"
        }
    }

    sourceSets.getByName("safetynet").manifest.srcFile("src/nightly/AndroidManifest.xml")

    flavorDimensions += "distribution"

    productFlavors {
        create("gp") { dimension = "distribution" }
        create("vanilla") { dimension = "distribution" }
    }
}

dependencies {
    implementation(libs.hilt.androidx.work)
    ksp(libs.hilt.androidx.compiler)

    implementation(libs.androidx.appcompat)

    implementation(libs.bundles.androidx.navigation)

    implementation(libs.chrisbanes.insetter)

    implementation(libs.kirich.viewbinding)

    implementation(libs.coil.kt)

    implementation(project(":common"))
    implementation(project(":design"))
    implementation(project(":chains"))
    implementation(project(":database"))

    // Region features
    implementation(project(":feature:backup:impl"))
    implementation(project(":feature:xcm:impl"))
    implementation(project(":feature:fund:impl"))
    implementation(project(":feature:swap:impl"))
    implementation(project(":feature:chats:impl"))
    implementation(project(":feature:device-sync:impl"))
    implementation(project(":feature:people:impl"))
    implementation(project(":feature:members:impl"))
    implementation(project(":feature:wallet:impl"))
    implementation(project(":feature:tokens:impl"))
    implementation(project(":feature:splash:impl"))
    implementation(project(":feature:prices:impl"))
    implementation(project(":feature:account:impl"))
    implementation(project(":feature:settings:impl"))
    implementation(project(":feature:balances:impl"))
    implementation(project(":feature:transfers:impl"))
    implementation(project(":feature:usernames:impl"))
    implementation(project(":feature:videogame:impl"))
    implementation(project(":feature:transactions:impl"))
    implementation(project(":feature:statement-store:impl"))
    implementation(project(":feature:transaction-storage:impl"))
    implementation(project(":feature:pgas:impl"))
    implementation(project(":feature:chain-resources:impl"))
    implementation(project(":feature:cross-chain-transfers:impl"))
    implementation(project(":feature:sso:impl"))
    implementation(project(":feature:scan:impl"))
    implementation(project(":feature:products:impl"))
    implementation(project(":feature:calls:api"))
    implementation(project(":feature:calls:impl"))
    implementation(project(":feature:coinage:impl"))
    implementation(project(":feature:dotns:impl"))
    implementation(project(":feature:dotns-gateway:impl"))
    implementation(project(":feature:connection-status:api"))
    implementation(project(":feature:connection-status:impl"))
    implementation(project(":feature:revive:impl"))
    implementation(project(":feature:w3s-pay:impl"))
    // Endregion features

    // Region tools
    implementation(project(":tools:auth:impl"))
    implementation(project(":tools:backup:impl"))
    implementation(project(":tools:integrity:impl"))
    implementation(project(":tools:jwt-auth:impl"))
    implementation(project(":tools:biometrics:impl"))
    implementation(project(":tools:remoteconfig:impl"))
    implementation(project(":tools:ipfs:impl"))
    implementation(project(":tools:assethub-sdk:impl"))
    implementation(project(":tools:hydration-sdk:impl"))
    implementation(project(":tools:push-notifications:impl"))
    implementation(project(":tools:media-connection:impl"))
    implementation(project(":tools:media-connection:api"))
    // Endregion tools

    "gpImplementation"(platform(libs.firebase.bom))
    "gpImplementation"(libs.firebase.crashlytics)
    "gpImplementation"(libs.firebase.analytics)

    "gpImplementation"(libs.google.play.services.mlkit)
    "vanillaImplementation"(libs.google.mlkit.barcode.scanning)

    testImplementation(project(":test-shared"))
    testImplementation(libs.squareup.okhttp3.mockwebserver)
    testImplementation(libs.squareup.okhttp3.tls)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(project(":bindings:truapi-host"))
    androidTestImplementation(libs.androidx.work.testing)
    androidTestImplementation(libs.hilt.android.testing)
    androidTestImplementation(libs.mockk.android) {
        // JUnit 4 runs these tests; mockk's JUnit 5 jars only collide when the test APK is packaged.
        exclude(group = "org.junit.jupiter")
        exclude(group = "org.junit.platform")
    }
    kspAndroidTest(libs.hilt.android.compiler)
}

sentry {
    org.set(localProperties.readSecretOrThrow("SENTRY_ORG"))
    projectName.set(localProperties.readSecretOrThrow("SENTRY_PROJECT"))

    // this will upload your source code to Sentry to show it as part of the stack traces
    // disable if you don't want to expose your sources
    includeSourceContext.set(true)

    // Sentry only runs on debug/nightly; skip release so the plugin
    // doesn't try to process a variant that has no DSN/auth token
    ignoredBuildTypes.set(setOf("release"))
}
