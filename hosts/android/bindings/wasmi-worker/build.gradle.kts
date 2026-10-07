plugins {
    id("polkadotapp.android.rust")
}

android {
    namespace = "io.paritytech.polkadotapp.wasmi_worker"
}

cargo {
    libname = "wasmi_worker_java"
}

dependencies {
    implementation(project(":common"))

    testImplementation(libs.junit)
}
