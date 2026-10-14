plugins {
    id("polkadotapp.android.rust")
}

android {
    namespace = "io.paritytech.polkadotapp.hydra_dx_math"
}

cargo {
    libname = "hydra_dx_math_java"
}

dependencies {
    implementation(project(":common"))

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
}
