import AppKit
import AuthenticationServices
import SwiftUI

struct CloudflareOAuthConnectorView: View {
    @Binding private var accountID: String
    @Binding private var clientID: String
    @Binding private var scopesText: String
    @Binding private var redirectURI: String

    private let showsConfigurationFields: Bool
    private let showsRegistrationButton: Bool
    private let showsHelpText: Bool
    private let showsDisconnectButton: Bool
    private let apiTokenProvider: () -> String?
    private let onConfigurationApplied: (String, String, String, String) -> Void
    private let onCredentialChanged: () -> Void

    @State private var credentialSource: ProviderCredentialSource = .none
    @State private var isConnecting = false
    @State private var isRegistering = false
    @State private var statusMessage: String?
    @State private var authSession: ASWebAuthenticationSession?
    @State private var presentationProvider = CloudflareOAuthPresentationProvider()

    init(
        accountID: Binding<String>,
        clientID: Binding<String>,
        scopesText: Binding<String>,
        redirectURI: Binding<String>,
        showsConfigurationFields: Bool = true,
        showsRegistrationButton: Bool = true,
        showsHelpText: Bool = true,
        showsDisconnectButton: Bool = true,
        apiTokenProvider: @escaping () -> String? = {
            ProviderCredentialStore().resolvedAccessToken(for: .cloudflare)
        },
        onConfigurationApplied: @escaping (String, String, String, String) -> Void = { _, _, _, _ in },
        onCredentialChanged: @escaping () -> Void = {}
    ) {
        self._accountID = accountID
        self._clientID = clientID
        self._scopesText = scopesText
        self._redirectURI = redirectURI
        self.showsConfigurationFields = showsConfigurationFields
        self.showsRegistrationButton = showsRegistrationButton
        self.showsHelpText = showsHelpText
        self.showsDisconnectButton = showsDisconnectButton
        self.apiTokenProvider = apiTokenProvider
        self.onConfigurationApplied = onConfigurationApplied
        self.onCredentialChanged = onCredentialChanged
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            if showsConfigurationFields {
                TextField("Cloudflare account ID", text: $accountID)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                TextField("OAuth client ID", text: $clientID)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                TextField("OAuth scopes", text: $scopesText)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    TextField("Redirect URI", text: $redirectURI)
                        .textFieldStyle(.roundedBorder)
                        .aetowerUtilityTextInput()
                    Button {
                        copyRedirectURI()
                    } label: {
                        Label("Copy", systemImage: "doc.on.doc")
                    }
                }
                .controlSize(.small)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                if showsRegistrationButton {
                    Button {
                        registerClient()
                    } label: {
                        Label("Register OAuth client", systemImage: "plus.circle")
                    }
                    .disabled(isConnecting || isRegistering)
                }

                Button {
                    connect()
                } label: {
                    Label("Connect Cloudflare", systemImage: "cloud")
                }
                .disabled(isConnecting || isRegistering)

                if isConnecting || isRegistering {
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

            if let statusMessage {
                Text(statusMessage)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }

            if showsHelpText {
                Text("Use a public Cloudflare OAuth client with PKCE and Pages/Workers read scopes.")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }
        }
        .onAppear(perform: reloadCredentialSource)
        .onDisappear {
            authSession?.cancel()
            authSession = nil
            isConnecting = false
            isRegistering = false
        }
    }

    private var normalizedAccountID: String {
        accountID.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var normalizedClientID: String {
        clientID.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var normalizedScopesText: String {
        SettingsStore.normalizedOAuthScopes(scopesText)
    }

    private var normalizedRedirectURI: String {
        SettingsStore.normalizedCloudflareOAuthRedirectURI(redirectURI)
    }

    private var scopes: [String] {
        normalizedScopesText
            .split(whereSeparator: \.isWhitespace)
            .map(String.init)
    }

    private func connect() {
        let appliedClientID = normalizedClientID
        let appliedAccountID = normalizedAccountID
        let appliedScopesText = normalizedScopesText
        let appliedRedirectURI = normalizedRedirectURI
        accountID = appliedAccountID
        clientID = appliedClientID
        scopesText = appliedScopesText
        redirectURI = appliedRedirectURI

        guard !appliedClientID.isEmpty else {
            statusMessage = "Set a Cloudflare OAuth client ID first."
            return
        }
        guard let callbackScheme = URL(string: appliedRedirectURI)?.scheme,
              !callbackScheme.isEmpty
        else {
            statusMessage = "Set a valid Cloudflare OAuth redirect URI first."
            return
        }

        onConfigurationApplied(
            appliedAccountID,
            appliedClientID,
            appliedScopesText,
            appliedRedirectURI
        )
        do {
            let pkce = try CloudflareOAuthPKCEGenerator.make()
            let client = CloudflareOAuthPKCEClient.live
            let authURL = try client.authorizationURL(
                clientID: appliedClientID,
                redirectURI: appliedRedirectURI,
                scopes: scopes,
                state: pkce.state,
                codeChallenge: pkce.codeChallenge
            )
            startAuthenticationSession(
                url: authURL,
                callbackScheme: callbackScheme,
                clientID: appliedClientID,
                redirectURI: appliedRedirectURI,
                requestedScopes: scopes,
                pkce: pkce,
                client: client
            )
        } catch {
            statusMessage = error.localizedDescription
            isConnecting = false
        }
    }

    private func registerClient() {
        let appliedAccountID = normalizedAccountID
        let appliedScopesText = normalizedScopesText
        let appliedRedirectURI = normalizedRedirectURI
        accountID = appliedAccountID
        scopesText = appliedScopesText
        redirectURI = appliedRedirectURI

        guard !appliedAccountID.isEmpty else {
            statusMessage = "Set a Cloudflare account ID first."
            return
        }
        guard URL(string: appliedRedirectURI)?.scheme?.isEmpty == false else {
            statusMessage = "Set a valid Cloudflare OAuth redirect URI first."
            return
        }
        guard let apiToken = apiTokenProvider()?.trimmingCharacters(in: .whitespacesAndNewlines),
              !apiToken.isEmpty
        else {
            statusMessage = "Set a Cloudflare API token first."
            return
        }

        isRegistering = true
        statusMessage = "Registering Cloudflare OAuth client."
        Task { @MainActor in
            defer { isRegistering = false }
            do {
                let registeredClientID = try await CloudflareOAuthPKCEClient.live
                    .registerOAuthClient(
                        accountID: appliedAccountID,
                        apiToken: apiToken,
                        redirectURI: appliedRedirectURI,
                        scopes: scopes
                    )
                clientID = registeredClientID
                onConfigurationApplied(
                    appliedAccountID,
                    registeredClientID,
                    appliedScopesText,
                    appliedRedirectURI
                )
                statusMessage = "Registered Cloudflare OAuth client."
            } catch {
                statusMessage = error.localizedDescription
            }
        }
    }

    private func startAuthenticationSession(
        url: URL,
        callbackScheme: String,
        clientID: String,
        redirectURI: String,
        requestedScopes: [String],
        pkce: CloudflareOAuthPKCE,
        client: CloudflareOAuthPKCEClient
    ) {
        authSession?.cancel()
        isConnecting = true
        statusMessage = "Opening Cloudflare authorization."
        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: callbackScheme
        ) { callbackURL, error in
            Task { @MainActor in
                authSession = nil
                if let error {
                    statusMessage = error.localizedDescription
                    isConnecting = false
                    return
                }
                guard let callbackURL else {
                    statusMessage = "Cloudflare OAuth returned no callback URL."
                    isConnecting = false
                    return
                }
                await handleCallback(
                    callbackURL,
                    clientID: clientID,
                    redirectURI: redirectURI,
                    requestedScopes: requestedScopes,
                    pkce: pkce,
                    client: client
                )
            }
        }
        session.presentationContextProvider = presentationProvider
        session.prefersEphemeralWebBrowserSession = false
        authSession = session
        if !session.start() {
            authSession = nil
            isConnecting = false
            statusMessage = "Cloudflare OAuth browser session could not start."
        }
    }

    private func handleCallback(
        _ callbackURL: URL,
        clientID: String,
        redirectURI: String,
        requestedScopes: [String],
        pkce: CloudflareOAuthPKCE,
        client: CloudflareOAuthPKCEClient
    ) async {
        let callback = CloudflareOAuthPKCEClient.parseCallbackURL(callbackURL)
        guard callback.state == pkce.state else {
            statusMessage = "Cloudflare OAuth state did not match."
            isConnecting = false
            return
        }
        if let error = callback.error {
            statusMessage = callback.errorDescription ?? "Cloudflare OAuth failed: \(error)."
            isConnecting = false
            return
        }
        guard let code = callback.code, !code.isEmpty else {
            statusMessage = "Cloudflare OAuth returned no authorization code."
            isConnecting = false
            return
        }

        statusMessage = "Exchanging Cloudflare authorization code."
        do {
            let token = try await client.exchangeCode(
                clientID: clientID,
                code: code,
                redirectURI: redirectURI,
                codeVerifier: pkce.codeVerifier
            )
            try await store(token, requestedScopes: requestedScopes, client: client)
        } catch {
            statusMessage = error.localizedDescription
        }
        isConnecting = false
    }

    private func store(
        _ token: CloudflareOAuthAccessToken,
        requestedScopes: [String],
        client: CloudflareOAuthPKCEClient
    ) async throws {
        let identity = try? await client.fetchUserIdentity(accessToken: token.accessToken)
        let capturedAtMillis = ProviderOAuthCredential.currentMillis()
        let credential = ProviderOAuthCredential(
            accessToken: token.accessToken,
            refreshToken: token.refreshToken,
            expiresAtMillis: token.expiresAtMillis(capturedAtMillis: capturedAtMillis),
            scopes: token.scopes.isEmpty ? requestedScopes : token.scopes,
            accountIdentity: identity,
            capturedAtMillis: capturedAtMillis
        )
        let stored = ProviderCredentialStore().storeOAuthCredential(credential, for: .cloudflare)
        guard stored else {
            statusMessage = "Could not store Cloudflare OAuth token in Keychain."
            return
        }
        statusMessage = identity.map { "Connected to Cloudflare as \($0)." }
            ?? "Connected to Cloudflare."
        reloadCredentialSource()
        onCredentialChanged()
    }

    private func disconnect() {
        authSession?.cancel()
        authSession = nil
        isConnecting = true
        statusMessage = "Disconnecting Cloudflare OAuth."
        let accessToken = ProviderCredentialStore()
            .oauthCredential(for: .cloudflare)?
            .accessToken
        let clientID = normalizedClientID

        Task { @MainActor in
            defer {
                ProviderCredentialStore().clearOAuthCredential(for: .cloudflare)
                reloadCredentialSource()
                onCredentialChanged()
                isConnecting = false
            }
            guard let accessToken, !accessToken.isEmpty, !clientID.isEmpty else {
                statusMessage = "Cloudflare OAuth disconnected."
                return
            }
            do {
                try await CloudflareOAuthPKCEClient.live.revoke(
                    accessToken: accessToken,
                    clientID: clientID
                )
                statusMessage = "Cloudflare OAuth disconnected."
            } catch {
                statusMessage = "Cloudflare OAuth cleared locally; revoke failed: \(error.localizedDescription)"
            }
        }
    }

    private func reloadCredentialSource() {
        credentialSource = ProviderCredentialStore().credentialSource(for: .cloudflare)
    }

    private func copyRedirectURI() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(normalizedRedirectURI, forType: .string)
    }
}

private final class CloudflareOAuthPresentationProvider: NSObject,
    ASWebAuthenticationPresentationContextProviding
{
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        NSApplication.shared.keyWindow
            ?? NSApplication.shared.windows.first
            ?? NSWindow()
    }
}
