import SwiftUI

public enum HolographicShaders {
    public static func iridescentShine(size: CGSize, tilt: CGPoint) -> Shader {
        ShaderLibrary.bundle(.module).holographicGradient(
            .float2(Float(size.width), Float(size.height)),
            .float2(Float(tilt.x), Float(tilt.y))
        )
    }
}
