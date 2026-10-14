import org.gradle.api.Project

private const val DefaultVersionName = "1.0.0"
private const val DefaultVersionCode = 28

fun Project.computeVersionName(): String = DefaultVersionName
fun Project.computeVersionCode(): Int = System.getenv("CI_BUILD_ID")?.toIntOrNull() ?: DefaultVersionCode
