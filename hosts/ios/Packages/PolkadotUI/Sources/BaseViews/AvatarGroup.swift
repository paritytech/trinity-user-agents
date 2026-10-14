import SwiftUI

struct AvatarGroup: View {
    let viewModels: [AvatarViewModel]
    let size: CGFloat
    let overlap: CGFloat

    init(
        viewModels: [AvatarViewModel],
        size: CGFloat,
        overlap: CGFloat
    ) {
        self.viewModels = viewModels
        self.size = size
        self.overlap = overlap
    }

    var body: some View {
        HStack(spacing: -overlap) {
            ForEach(Array(viewModels.enumerated()), id: \.offset) { _, viewModel in
                AvatarViewSUI(viewModel: viewModel, size: size)
                    .frame(width: size, height: size)
                    .overlay(
                        Circle().stroke(.foreground, lineWidth: 1)
                    )
            }
        }
        .frame(height: size)
    }
}

#Preview {
    AvatarGroup(
        viewModels: [
            .image(.add),
            .image(.add),
            .image(.add)
        ],
        size: 24,
        overlap: 8
    )
}
