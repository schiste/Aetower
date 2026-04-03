import SwiftUI

extension View {
    @ViewBuilder
    func aetowerUtilityTextInput() -> some View {
        if #available(macOS 15.0, *) {
            self
                .writingToolsBehavior(.disabled)
                .autocorrectionDisabled()
        } else {
            self
        }
    }
}
