plugins {
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
}

android {
    namespace = "io.paritytech.polkadotapp.design"
}

dependencies {
    implementation(libs.hilt.android)
    ksp(libs.hilt.android.compiler)

    api(platform(libs.androidx.compose.bom))
    androidTestApi(platform(libs.androidx.compose.bom))

    api(libs.androidx.compose.ui)
    api(libs.androidx.compose.foundation)
    api(libs.androidx.compose.material3)
    api(libs.androidx.compose.animation.graphics)
    api(libs.androidx.compose.ui.tooling.preview)
    api(libs.androidx.compose.ui.util)
    api(libs.androidx.lifecycle.runtime.compose)

    api(libs.androidx.activity.compose)
    api(libs.androidx.constraintlayout.compose)
    api(libs.alexzhirkevich.qrose)

    api(libs.coil.compose)

    api(libs.blurhash)

    api(libs.polkadot.design.system)

    api(libs.kotlinx.collections.immutable)

    debugApi(libs.androidx.compose.ui.tooling)
    nightlyApi(libs.androidx.compose.ui.tooling)
    "safetynetApi"(libs.androidx.compose.ui.tooling)

    testApi(libs.androidx.compose.ui.test)
    debugImplementation(libs.androidx.compose.material.icons.extended)
    nightlyImplementation(libs.androidx.compose.material.icons.extended)
    "safetynetImplementation"(libs.androidx.compose.material.icons.extended)
}
