import AppKit
import SwiftUI

struct GitHubOAuthConnectorView: View {
    @Binding private var clientID: String
    @Binding private var scopesText: String

    private let showsConfigurationFields: Bool
    private let showsHelpText: Bool
    private let showsDisconnectButton: Bool
    private let onConfigurationApplied: (String, String) -> Void
    private let onCredentialChanged: () -> Void

    @State private var credentialSource: ProviderCredentialSource = .none
    @State private var isConnecting = false
    @State private var userCode: String?
    @State private var verificationURI: String?
    @State private var statusMessage: String?
    @State private var oauthTask: Task<Void, Never>?

    init(
        clientID: Binding<String>,
        scopesText: Binding<String>,
        showsConfigurationFields: Bool = true,
        showsHelpText: Bool = true,
        showsDisconnectButton: Bool = true,
        onConfigurationApplied: @escaping (String, String) -> Void = { _, _ in },
        onCredentialChanged: @escaping () -> Void = {}
    ) {
        self._clientID = clientID
        self._scopesText = scopesText
        self.showsConfigurationFields = showsConfigurationFields
        self.showsHelpText = showsHelpText
        self.showsDisconnectButton = showsDisconnectButton
        self.onConfigurationApplied = onConfigurationApplied
        self.onCredentialChanged = onCredentialChanged
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            if showsConfigurationFields {
                TextField("OAuth client ID", text: $clientID)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                TextField("OAuth scopes", text: $scopesText)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button {
                    connect()
                } label: {
                    Label("Connect GitHub", systemImage: "person.crop.circle.badge.checkmark")
                }
                .disabled(isConnecting)

                if isConnecting {
                    ProgressView()
                        .controlSize(.small)
                }

                if showsDisconnectButton && credentialSource == .oauth {
                    Button {
                        disconnect()
                    } label: {
                        Label("Disconnect OAuth", systemImage: "xmark.circle")
                    }
                }
            }
            .controlSize(.small)

            if let userCode, let verificationURI {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text(userCode)
                        .font(.caption.monospacedDigit().weight(.semibold))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Button {
                        copyUserCode()
                    } label: {
                        Label("Copy", systemImage: "doc.on.doc")
                    }
                    Button {
                        openDeviceURL()
                    } label: {
                        Label("Open", systemImage: "safari")
                    }
                }
                .controlSize(.small)
                Text("Enter the code at \(verificationURI).")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }

            if let statusMessage {
                Text(statusMessage)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }

            if showsHelpText {
                Text(
                    "OAuth client ID is public. Leave scopes blank for public data, or request broader scopes only when needed."
                )
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.secondary)
            }
        }
        .onAppear(perform: reloadCredentialSource)
        .onDisappear {
            oauthTask?.cancel()
            oauthTask = nil
            isConnecting = false
        }
    }

    private var normalizedClientID: String {
        clientID.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var normalizedScopesText: String {
        SettingsStore.normalizedOAuthScopes(scopesText)
    }

    private var scopes: [String] {
        normalizedScopesText
            .split(whereSeparator: \.isWhitespace)
            .map(String.init)
    }

    private func connect() {
        let appliedClientID = normalizedClientID
        let appliedScopesText = normalizedScopesText
        clientID = appliedClientID
        scopesText = appliedScopesText

        guard !appliedClientID.isEmpty else {
            statusMessage = "Set a GitHub OAuth client ID first."
            return
        }

        onConfigurationApplied(appliedClientID, appliedScopesText)
        oauthTask?.cancel()
        isConnecting = true
        userCode = nil
        verificationURI = nil
        statusMessage = "Requesting GitHub device code."
        let requestedScopes = scopes
        let client = GitHubOAuthDeviceClient.live

        oauthTask = Task { @MainActor in
            defer {
                isConnecting = false
                oauthTask = nil
            }
            do {
                let deviceCode = try await client.requestDeviceCode(
                    clientID: appliedClientID,
                    scopes: requestedScopes
                )
                guard !Task.isCancelled else { return }
                userCode = deviceCode.userCode
                verificationURI = deviceCode.verificationURI
                statusMessage = "Waiting for GitHub authorization."
                openDeviceURL()
                try await poll(
                    client: client,
                    clientID: appliedClientID,
                    deviceCode: deviceCode
                )
            } catch is CancellationError {
                statusMessage = nil
            } catch {
                statusMessage = error.localizedDescription
            }
        }
    }

    private func poll(
        client: GitHubOAuthDeviceClient,
        clientID: String,
        deviceCode: GitHubOAuthDeviceCode
    ) async throws {
        var interval = max(1, deviceCode.interval)
        let expiresAt = Date().addingTimeInterval(TimeInterval(deviceCode.expiresIn))
        while !Task.isCancelled, Date() < expiresAt {
            try await Task.sleep(nanoseconds: UInt64(interval) * 1_000_000_000)
            let result = try await client.pollAccessToken(
                clientID: clientID,
                deviceCode: deviceCode.deviceCode
            )
            switch result {
            case .pending:
                statusMessage = "Waiting for GitHub authorization."
            case let .slowDown(newInterval):
                interval = max(interval + 5, newInterval ?? interval + 5)
                statusMessage = "GitHub asked Aetower to slow down polling."
            case let .authorized(token):
                try await store(token, client: client)
                return
            case let .denied(message), let .expired(message), let .failed(message):
                statusMessage = message
                return
            }
        }
        if !Task.isCancelled {
            statusMessage = "GitHub device code expired."
        }
    }

    private func store(
        _ token: GitHubOAuthAccessToken,
        client: GitHubOAuthDeviceClient
    ) async throws {
        let identity = try? await client.fetchUserIdentity(accessToken: token.accessToken)
        let credential = ProviderOAuthCredential(
            accessToken: token.accessToken,
            scopes: token.scopes,
            accountIdentity: identity,
            capturedAtMillis: ProviderOAuthCredential.currentMillis()
        )
        let stored = ProviderCredentialStore().storeOAuthCredential(credential, for: .github)
        guard stored else {
            statusMessage = "Could not store GitHub OAuth token in Keychain."
            return
        }
        userCode = nil
        verificationURI = nil
        statusMessage = identity.map { "Connected to GitHub as \($0)." }
            ?? "Connected to GitHub."
        reloadCredentialSource()
        onCredentialChanged()
    }

    private func disconnect() {
        oauthTask?.cancel()
        oauthTask = nil
        isConnecting = false
        userCode = nil
        verificationURI = nil
        ProviderCredentialStore().clearOAuthCredential(for: .github)
        reloadCredentialSource()
        onCredentialChanged()
        statusMessage = "GitHub OAuth disconnected."
    }

    private func reloadCredentialSource() {
        credentialSource = ProviderCredentialStore().credentialSource(for: .github)
    }

    private func copyUserCode() {
        guard let userCode else { return }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(userCode, forType: .string)
    }

    private func openDeviceURL() {
        let uri = verificationURI ?? "https://github.com/login/device"
        guard let url = URL(string: uri) else { return }
        NSWorkspace.shared.open(url)
    }
}
