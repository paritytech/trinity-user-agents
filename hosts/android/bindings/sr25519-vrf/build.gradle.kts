plugins {
    id("polkadotapp.android.rust")
}

android {
    namespace = "io.paritytech.polkadotapp.sr25519_vrf"
}

cargo {
    libname = "sr25519_vrf_java"
}

dependencies {
    implementation(project(":common"))

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    // Derives a valid 96-byte sr25519 keypair for the on-device known-answer test, the same way
    // production sources it from the wallet key.
    androidTestImplementation(libs.nova.substrate.sdk)
}
