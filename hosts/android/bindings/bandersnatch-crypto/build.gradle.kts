plugins {
    id("polkadotapp.android.rust")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.bandersnatch_crypto"
}

cargo {
    libname = "bandersnatch_crypto_java"
}

dependencies {
    implementation(project(":common"))

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
}
