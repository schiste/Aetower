import Foundation
import Observation
import Sparkle

@MainActor
@Observable
public final class UpdaterController {
    public private(set) var isConfigured = false
    public private(set) var statusMessage: String

    @ObservationIgnored
    private var updaterController: SPUStandardUpdaterController?

    public init(bundle: Bundle = .main) {
        let appcastURL = bundle.object(forInfoDictionaryKey: "SUFeedURL") as? String
        let publicKey = bundle.object(forInfoDictionaryKey: "SUPublicEDKey") as? String

        guard
            let appcastURL,
            !appcastURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            let publicKey,
            !publicKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else {
            self.statusMessage = "Updates are disabled until the release build includes a Sparkle feed URL and public key."
            self.isConfigured = false
            self.updaterController = nil
            return
        }

        self.updaterController = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)
        self.statusMessage = "Automatic update checks are enabled for direct-download releases."
        self.isConfigured = true
    }

    public func checkForUpdates() {
        updaterController?.checkForUpdates(nil)
    }
}
