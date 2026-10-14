package io.paritytech.polkadotapp.tools_car_parser

/**
 * The result of parsing and reconstructing a CAR archive — a flat map of file paths to their content.
 *
 * Paths are absolute within the archive root (e.g., "/index.html", "/assets/logo.png").
 */
@JvmInline
value class UnpackedCarArchive(val files: Map<FilePath, FileContent>)
