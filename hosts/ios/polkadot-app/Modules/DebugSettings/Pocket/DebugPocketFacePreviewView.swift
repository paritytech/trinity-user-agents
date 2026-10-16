import PolkadotUI
import SwiftUI

/// The face-preview loop: type a URL, draw what comes back in the card frame.
struct DebugPocketFacePreviewView: View {
    @State var viewModel = DebugPocketFacePreviewViewModel()

    var body: some View {
        VStack(spacing: 16) {
            TextField("Face URL", text: $viewModel.url)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .textFieldStyle(.roundedBorder)

            Button("Draw") { Task { await viewModel.load() } }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isLoading)

            // Drawn in the real card frame, so what is seen here is what the
            // Pocket will show. The face comes from a URL rather than from a
            // product's archive, so there is nothing for an archive path to
            // resolve against and those images draw as holes.
            PocketProductCardView(title: "Preview", face: viewModel.face, resolveImage: nil)

            if let refusal = viewModel.refusal {
                ScrollView {
                    Text(refusal)
                        .textStyle(.body14Regular())
                        .foregroundStyle(Color(.fgError))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
            }

            Spacer()
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color(.bgSurfaceMain))
        .navigationTitle("Pocket face preview")
    }
}
