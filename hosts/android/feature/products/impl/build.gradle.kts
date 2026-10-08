import java.util.Properties
plugins {
    id("jacoco")
    id("polkadotapp.android.library")
    id("polkadotapp.android.compose")
    id("polkadotapp.android.hilt")
    alias(libs.plugins.kotlin.parcelize)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "io.paritytech.polkadotapp.feature_products_impl"
}

val containerDir = rootProject.file("feature/products/product-container")
val containerOutput = containerDir.resolve("dist/container.js")
val assetsDir = project.file("src/main/assets")
val isWindows = org.gradle.internal.os.OperatingSystem.current().isWindows

fun Exec.runMultiplatformCommand(command: String) {
    if (isWindows) {
        commandLine("cmd", "/c", command)
    } else {
        commandLine("bash", "-lc", command)
    }
}

val npmInstallContainer by tasks.registering(Exec::class) {
    workingDir = containerDir
    runMultiplatformCommand("npm install")

    inputs.file(containerDir.resolve("package.json"))
    inputs.file(containerDir.resolve("package-lock.json"))
    outputs.dir(containerDir.resolve("node_modules"))
}

val buildContainerScript by tasks.registering(Exec::class) {
    dependsOn(npmInstallContainer)
    workingDir = containerDir
    runMultiplatformCommand("npm run build")

    inputs.dir(containerDir.resolve("src"))
    // Bundled deps are inlined by esbuild, so a version bump alone must invalidate the output.
    inputs.file(containerDir.resolve("package-lock.json"))
    outputs.file(containerOutput)
}

val copyContainerScript by tasks.registering(Copy::class) {
    dependsOn(buildContainerScript)
    from(containerOutput)
    into(assetsDir)
}

afterEvaluate {
    tasks.matching { it.name.startsWith("merge") && it.name.endsWith("Assets") }.configureEach {
        dependsOn(copyContainerScript)
    }
    tasks.matching { "lint" in it.name.lowercase() }.configureEach {
        dependsOn(copyContainerScript)
    }
}

dependencies {
    api(project(":feature:products:api"))

    implementation(libs.hilt.lifecycle.viewmodel.compose)
    implementation(libs.androidx.fragment.ktx)
    implementation(libs.androidx.webkit)

    implementation(libs.kotlinx.serialization.json)
    implementation(libs.nova.substrate.serialization)

    implementation(project(":bindings:truapi-host"))
    implementation(project(":bindings:sr25519-vrf"))

    implementation(project(":common"))
    implementation(project(":tools:ipfs:api"))
    implementation(project(":design"))
    implementation(project(":database"))
    implementation(project(":chains"))
    implementation(project(":feature:chats:api"))
    implementation(project(":feature:account:api"))
    implementation(project(":feature:settings:api"))
    implementation(project(":feature:transaction-storage:api"))
    implementation(project(":feature:transactions:api"))
    implementation(project(":feature:statement-store:api"))
    implementation(project(":feature:pgas:api"))
    implementation(project(":feature:balances:api"))
    implementation(project(":feature:people:api"))
    implementation(project(":feature:scan:api"))
    implementation(project(":feature:usernames:api"))
    implementation(project(":feature:dotns:api"))
    implementation(project(":feature:coinage:api"))

    implementation(libs.squareup.okhttp3.core)

    testImplementation(project(":test-shared"))
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.mockk)
    // :bindings:truapi-host ships JNA as an @aar, which carries only the Android
    // dispatch libraries. JVM unit tests that cross the FFI boundary need the
    // desktop jar's libjnidispatch too.
    testImplementation("net.java.dev.jna:jna:5.14.0")

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.google.gson)
}

// CoreNavigateClassifier calls the core's `parse_navigate` over JNA, so JVM unit
// tests need the host-native cdylib on the library path. It is the same artifact
// uniffi-bindgen reads, produced by :bindings:truapi-host:buildHostCdylib.
val truapiCodegenDir: String = run {
    val props = Properties().apply {
        val f = rootProject.file("local.properties")
        if (f.exists()) f.inputStream().use { load(it) }
    }
    val configured = (props.getProperty("truapi.dir") ?: System.getenv("TRUAPI_DIR"))
        ?.takeIf { it.isNotBlank() }
    // Falls back to the core two levels up when nothing is configured, which is
    // what the settings-gradle guard has already accepted. A configured path is
    // taken as given; the guard validated it.
    val dir = configured
        ?.let { file(it).takeIf { f -> f.isAbsolute } ?: rootProject.file(it) }
        ?: rootProject.file("../..")
    File(dir, "target/codegen").path
}

tasks.withType<Test>().configureEach {
    dependsOn(":bindings:truapi-host:buildHostCdylib")
    systemProperty("jna.library.path", truapiCodegenDir)

    // Instrument only when the coverage report is actually being built, so an ordinary test run is unaffected.
    extensions.configure<JacocoTaskExtension> {
        isEnabled = gradle.startParameter.taskNames.any { "topUpCoverage" in it }
    }
}

tasks.register<JacocoReport>("topUpCoverage") {
    dependsOn("testDebugUnitTest")

    executionData.setFrom(fileTree(layout.buildDirectory).matching { include("**/testDebugUnitTest.exec") })
    sourceDirectories.setFrom(files("src/main/java"))
    classDirectories.setFrom(
        fileTree(layout.buildDirectory.dir("tmp/kotlin-classes/debug")).matching {
            include("**/topUpRequest/**", "**/storage/TopUpSourceStorage*", "**/repository/TopUpRepository*")
            exclude("**/di/**", "**/*_Factory*", "**/*_HiltModules*", "**/hilt_aggregated_deps/**", "**/*Module*")
        }
    )

    reports {
        xml.required.set(true)
        html.required.set(true)
    }
}
