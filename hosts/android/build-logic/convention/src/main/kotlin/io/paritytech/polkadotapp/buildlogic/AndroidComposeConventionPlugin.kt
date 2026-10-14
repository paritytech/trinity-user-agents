package io.paritytech.polkadotapp.buildlogic

import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.configure
import org.jetbrains.kotlin.compose.compiler.gradle.ComposeCompilerGradlePluginExtension

class AndroidComposeConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) = with(target) {
        pluginManager.apply("org.jetbrains.kotlin.plugin.compose")

        extensions.configure<ComposeCompilerGradlePluginExtension> {
            stabilityConfigurationFiles.add(
                rootProject.layout.projectDirectory.file("config/compose/stability.conf")
            )

            if (providers.gradleProperty("composeReports").isPresent) {
                val dir = layout.buildDirectory.dir("compose_compiler")
                reportsDestination.set(dir)
                metricsDestination.set(dir)
            }
        }
    }
}
