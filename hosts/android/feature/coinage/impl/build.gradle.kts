plugins {
    id("jacoco")
    id("polkadotapp.android.library")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_coinage_impl"
}

dependencies {
    api(project(":feature:coinage:api"))
    api(project(":feature:transactions:api"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:balances:api"))
    implementation(project(":feature:members:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:pgas:api"))
    implementation(project(":feature:revive:api"))
    implementation(project(":feature:tokens:api"))
    implementation(project(":feature:transfers:api"))
    implementation(project(":feature:usernames:api"))

    implementation(project(":tools:remoteconfig:api"))

    implementation(project(":database"))

    implementation(libs.hilt.androidx.work)
    ksp(libs.hilt.androidx.compiler)

    implementation(libs.androidx.work.runtime)
    implementation(libs.web3j.abi)

    // Test-only: the harness builds the durability engine directly, and the rules shim calls its ladder.
    // Production coinage depends on the api alone, so this adds no impl-to-impl edge to the app graph.
    testImplementation(project(":feature:transactions:impl"))
    testImplementation(project(":test-shared"))
    testImplementation(libs.mockk)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    testImplementation(libs.kotlinx.coroutines.test)
}

// Per-test stdout in the XML report, so a run can be read back as "which rule decided which test".
tasks.withType<Test>().configureEach {
    reports.junitXml.isOutputPerTestCase = true

    // `org.gradle.jvmargs` sizes the daemon, not this forked JVM, which otherwise takes Gradle's small
    // default. A fuzz run fills that in a few hundred walks and then spends most of its time collecting.
    maxHeapSize = "4g"

    // Instrument only when the coverage report is actually being built: a long fuzz run under the agent
    // exhausts its instrumentation tables and takes the test JVM down with it, mid-walk and without a report.
    extensions.configure<JacocoTaskExtension> {
        isEnabled = gradle.startParameter.taskNames.any { "coinageCoverage" in it }
    }

    // Lets a long fuzz run be asked for without editing the test:
    //   ./gradlew :feature:coinage:impl:testDebugUnitTest --tests '*CoinageFuzzTest*' \
    //     -Dcoinage.fuzz.seeds=5000 -Dcoinage.fuzz.steps=400
    val fuzzProperties = listOf("coinage.fuzz.seeds", "coinage.fuzz.steps")
    fuzzProperties.forEach { property ->
        System.getProperty(property)?.let { systemProperty(property, it) }
    }

    // A run long enough to need watching is always asked for with one of those properties; the ordinary
    // suite stays quiet, where per-test stdout belongs in the XML report and not on the console.
    testLogging.showStandardStreams = fuzzProperties.any { System.getProperty(it) != null }
}

tasks.register<JacocoReport>("coinageCoverage") {
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
