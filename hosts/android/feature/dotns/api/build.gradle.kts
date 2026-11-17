plugins {
    id("polkadotapp.android.library")
}

android {
    namespace = "io.paritytech.polkadotapp.feature_dotns_api"
}

dependencies {
    implementation(project(":bindings:truapi-host"))
    api(project(":common"))

    testImplementation(libs.junit)
}

apply(from = rootProject.file("gradle/truapi-unit-tests.gradle.kts"))
