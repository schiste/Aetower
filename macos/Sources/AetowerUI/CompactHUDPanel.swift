import AppKit
import SwiftUI
import AetowerBridge

@MainActor
public final class CompactHUDPanel {
    private var panel: NSPanel?
    private let state: AppState

    public init(state: AppState) {
        self.state = state
    }

    public var isVisible: Bool {
        panel?.isVisible ?? false
    }

    public func toggle() {
        if let panel, panel.isVisible {
            panel.orderOut(nil)
        } else {
            show()
        }
    }

    private func show() {
        if panel == nil {
            let hosting = NSHostingView(rootView: CompactHUDView(state: state))
            hosting.frame = NSRect(x: 0, y: 0, width: 220, height: 300)

            let p = NSPanel(
                contentRect: hosting.frame,
                styleMask: [.nonactivatingPanel, .titled, .closable, .fullSizeContentView],
                backing: .buffered,
                defer: false
            )
            p.isFloatingPanel = true
            p.level = .floating
            p.titleVisibility = .hidden
            p.titlebarAppearsTransparent = true
            p.isMovableByWindowBackground = true
            p.isOpaque = false
            p.backgroundColor = .clear
            p.hasShadow = true
            p.contentView = hosting
            p.setContentSize(hosting.fittingSize)

            // Position in top-right corner
            if let screen = NSScreen.main {
                let x = screen.visibleFrame.maxX - hosting.fittingSize.width - 20
                let y = screen.visibleFrame.maxY - hosting.fittingSize.height - 20
                p.setFrameOrigin(NSPoint(x: x, y: y))
            }

            panel = p
        }

        panel?.orderFront(nil)
    }
}
