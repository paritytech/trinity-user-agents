plugins {
    id("jacoco")
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.parcelize)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_chats_impl"
}

dependencies {
    api(project(":feature:chats:api"))
    implementation(project(":feature:chats:transport-protocol"))

    ksp(libs.hilt.androidx.compiler)

    implementation(libs.hilt.androidx.work)
    implementation(libs.androidx.work.runtime)
    implementation(libs.androidx.exifinterface)

    implementation(libs.bundles.androidx.media3)

    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)

    implementation(project(":database"))
    implementation(project(":chains"))

    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:wallet:api"))
    implementation(project(":feature:transfers:api"))
    implementation(project(":feature:chain-resources:api"))
    implementation(project(":feature:statement-store:api"))
    implementation(project(":feature:calls:api"))
    implementation(project(":feature:coinage:api"))
    implementation(project(":feature:sso:api"))
    implementation(project(":feature:transaction-storage:api"))
    implementation(project(":feature:scan:api"))

    implementation(project(":tools:push-notifications:api"))
    implementation(project(":tools:ipfs:api"))

    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)
    testImplementation(libs.kotlinx.coroutines.test)
}

tasks.register<JacocoReport>("chatsCoverage") {
    dependsOn("testDebugUnitTest")

    executionData.setFrom(fileTree(layout.buildDirectory).matching { include("**/testDebugUnitTest.exec") })
    sourceDirectories.setFrom(files("src/main/java"))
    classDirectories.setFrom(
        fileTree(layout.buildDirectory.dir("tmp/kotlin-classes/debug")).matching {
            exclude("**/di/**", "**/*_Factory*", "**/*_HiltModules*", "**/hilt_aggregated_deps/**", "**/*Module*")
        }
    )

    reports {
        xml.required.set(true)
        html.required.set(true)
    }
}
