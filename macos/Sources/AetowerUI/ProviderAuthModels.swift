import Foundation

public enum ProviderCredentialSource: String, Codable, CaseIterable, Identifiable, Sendable {
    case none
    case manualToken = "manual_token"
    case oauth

    public var id: String { rawValue }
}

public enum ProjectProvider: String, Codable, CaseIterable, Identifiable, Sendable {
    case github
    case cloudflare

    public var id: String { rawValue }

    var manualTokenAccount: String {
        switch self {
        case .github: return KeychainHelper.githubProviderTokenAccount
        case .cloudflare: return KeychainHelper.cloudflareProviderTokenAccount
        }
    }

    var oauthAccessTokenAccount: String {
        "provider.oauth.\(rawValue).accessToken"
    }

    var oauthRefreshTokenAccount: String {
        "provider.oauth.\(rawValue).refreshToken"
    }
}

public struct ProviderOAuthCredential: Equatable, Sendable {
    public var accessToken: String
    public var refreshToken: String?
    public var expiresAtMillis: UInt64?
    public var scopes: [String]
    public var accountIdentity: String?
    public var capturedAtMillis: UInt64

    public init(
        accessToken: String,
        refreshToken: String? = nil,
        expiresAtMillis: UInt64? = nil,
        scopes: [String] = [],
        accountIdentity: String? = nil,
        capturedAtMillis: UInt64 = Self.currentMillis()
    ) {
        self.accessToken = accessToken.trimmingCharacters(in: .whitespacesAndNewlines)
        self.refreshToken = Self.normalizedOptional(refreshToken)
        self.expiresAtMillis = expiresAtMillis
        self.scopes = Self.normalizedScopes(scopes)
        self.accountIdentity = Self.normalizedOptional(accountIdentity)
        self.capturedAtMillis = capturedAtMillis
    }

    public func isExpired(nowMillis: UInt64 = Self.currentMillis()) -> Bool {
        guard let expiresAtMillis else { return false }
        return nowMillis >= expiresAtMillis
    }

    private static func normalizedOptional(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func normalizedScopes(_ scopes: [String]) -> [String] {
        var seen = Set<String>()
        return scopes.compactMap { scope in
            let trimmed = scope.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty, seen.insert(trimmed).inserted else { return nil }
            return trimmed
        }
    }

    public static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

public struct ProviderOAuthCredentialMetadata: Codable, Equatable, Sendable {
    public var expiresAtMillis: UInt64?
    public var scopes: [String]
    public var accountIdentity: String?
    public var capturedAtMillis: UInt64
    public var hasRefreshToken: Bool

    public init(
        expiresAtMillis: UInt64? = nil,
        scopes: [String] = [],
        accountIdentity: String? = nil,
        capturedAtMillis: UInt64,
        hasRefreshToken: Bool
    ) {
        self.expiresAtMillis = expiresAtMillis
        self.scopes = scopes
        self.accountIdentity = accountIdentity
        self.capturedAtMillis = capturedAtMillis
        self.hasRefreshToken = hasRefreshToken
    }

    init(credential: ProviderOAuthCredential) {
        self.init(
            expiresAtMillis: credential.expiresAtMillis,
            scopes: credential.scopes,
            accountIdentity: credential.accountIdentity,
            capturedAtMillis: credential.capturedAtMillis,
            hasRefreshToken: credential.refreshToken != nil
        )
    }

    private enum CodingKeys: String, CodingKey {
        case expiresAtMillis = "expires_at_millis"
        case scopes
        case accountIdentity = "account_identity"
        case capturedAtMillis = "captured_at_millis"
        case hasRefreshToken = "has_refresh_token"
    }
}

public struct ProviderCredentialMetadata: Codable, Equatable, Sendable {
    public var source: ProviderCredentialSource
    public var oauth: ProviderOAuthCredentialMetadata?

    public init(
        source: ProviderCredentialSource = .none,
        oauth: ProviderOAuthCredentialMetadata? = nil
    ) {
        self.source = source
        self.oauth = oauth
    }
}

public struct ResolvedProviderCredential: Equatable, Sendable {
    public var source: ProviderCredentialSource
    public var accessToken: String

    public init(source: ProviderCredentialSource, accessToken: String) {
        self.source = source
        self.accessToken = accessToken
    }
}

public protocol ProviderCredentialSecretStoring {
    @discardableResult
    func store(_ value: String, account: String) -> Bool
    func retrieve(account: String) -> String?
    @discardableResult
    func delete(account: String) -> Bool
}

public struct KeychainProviderCredentialSecretStore: ProviderCredentialSecretStoring {
    public init() {}

    @discardableResult
    public func store(_ value: String, account: String) -> Bool {
        KeychainHelper.store(value, account: account)
    }

    public func retrieve(account: String) -> String? {
        KeychainHelper.retrieve(account: account)
    }

    @discardableResult
    public func delete(account: String) -> Bool {
        KeychainHelper.delete(account: account)
    }
}

public struct ProviderCredentialStore {
    public static let metadataDefaultsKey = "settings.providerCredentials.v1"

    private let defaults: UserDefaults
    private let secrets: ProviderCredentialSecretStoring

    public init(
        defaults: UserDefaults = .standard,
        secrets: ProviderCredentialSecretStoring = KeychainProviderCredentialSecretStore()
    ) {
        self.defaults = defaults
        self.secrets = secrets
    }

    public func credentialSource(for provider: ProjectProvider) -> ProviderCredentialSource {
        let metadata = metadata(for: provider)
        if metadata.source == .none, manualToken(for: provider) != nil {
            return .manualToken
        }
        return metadata.source
    }

    public func metadata(for provider: ProjectProvider) -> ProviderCredentialMetadata {
        allMetadata()[provider] ?? ProviderCredentialMetadata()
    }

    public func manualToken(for provider: ProjectProvider) -> String? {
        normalizedSecret(secrets.retrieve(account: provider.manualTokenAccount))
    }

    @discardableResult
    public func storeManualToken(_ token: String, for provider: ProjectProvider) -> Bool {
        let sanitized = token.trimmingCharacters(in: .whitespacesAndNewlines)
        let stored = secrets.store(sanitized, account: provider.manualTokenAccount)
        guard stored else { return false }

        var metadataByProvider = allMetadata()
        var metadata = metadataByProvider[provider] ?? ProviderCredentialMetadata()
        metadata.source = sanitized.isEmpty && oauthCredential(for: provider) != nil
            ? .oauth
            : sanitized.isEmpty ? .none : .manualToken
        metadataByProvider[provider] = metadata
        saveAllMetadata(metadataByProvider)
        return true
    }

    public func oauthCredential(for provider: ProjectProvider) -> ProviderOAuthCredential? {
        guard let metadata = metadata(for: provider).oauth,
              let accessToken = normalizedSecret(
                  secrets.retrieve(account: provider.oauthAccessTokenAccount)
              )
        else {
            return nil
        }
        let refreshToken = metadata.hasRefreshToken
            ? normalizedSecret(secrets.retrieve(account: provider.oauthRefreshTokenAccount))
            : nil
        return ProviderOAuthCredential(
            accessToken: accessToken,
            refreshToken: refreshToken,
            expiresAtMillis: metadata.expiresAtMillis,
            scopes: metadata.scopes,
            accountIdentity: metadata.accountIdentity,
            capturedAtMillis: metadata.capturedAtMillis
        )
    }

    @discardableResult
    public func storeOAuthCredential(
        _ credential: ProviderOAuthCredential,
        for provider: ProjectProvider
    ) -> Bool {
        guard !credential.accessToken.isEmpty else { return false }
        let accessStored = secrets.store(
            credential.accessToken,
            account: provider.oauthAccessTokenAccount
        )
        let refreshStored: Bool
        if let refreshToken = credential.refreshToken {
            refreshStored = secrets.store(refreshToken, account: provider.oauthRefreshTokenAccount)
        } else {
            refreshStored = secrets.delete(account: provider.oauthRefreshTokenAccount)
        }
        guard accessStored && refreshStored else { return false }

        var metadataByProvider = allMetadata()
        metadataByProvider[provider] = ProviderCredentialMetadata(
            source: .oauth,
            oauth: ProviderOAuthCredentialMetadata(credential: credential)
        )
        saveAllMetadata(metadataByProvider)
        return true
    }

    public func resolvedCredential(for provider: ProjectProvider) -> ResolvedProviderCredential? {
        switch credentialSource(for: provider) {
        case .oauth:
            guard let credential = oauthCredential(for: provider),
                  !credential.isExpired()
            else {
                return nil
            }
            return ResolvedProviderCredential(source: .oauth, accessToken: credential.accessToken)
        case .manualToken:
            guard let token = manualToken(for: provider) else { return nil }
            return ResolvedProviderCredential(source: .manualToken, accessToken: token)
        case .none:
            return nil
        }
    }

    public func resolvedAccessToken(for provider: ProjectProvider) -> String? {
        resolvedCredential(for: provider)?.accessToken
    }

    public func clear(_ provider: ProjectProvider) {
        secrets.delete(account: provider.manualTokenAccount)
        secrets.delete(account: provider.oauthAccessTokenAccount)
        secrets.delete(account: provider.oauthRefreshTokenAccount)
        var metadataByProvider = allMetadata()
        metadataByProvider[provider] = ProviderCredentialMetadata()
        saveAllMetadata(metadataByProvider)
    }

    public func clearOAuthCredential(for provider: ProjectProvider) {
        secrets.delete(account: provider.oauthAccessTokenAccount)
        secrets.delete(account: provider.oauthRefreshTokenAccount)
        var metadataByProvider = allMetadata()
        var metadata = metadataByProvider[provider] ?? ProviderCredentialMetadata()
        metadata.oauth = nil
        metadata.source = manualToken(for: provider) == nil ? .none : .manualToken
        metadataByProvider[provider] = metadata
        saveAllMetadata(metadataByProvider)
    }

    public func resetAll() {
        for provider in ProjectProvider.allCases {
            clear(provider)
        }
        defaults.removeObject(forKey: Self.metadataDefaultsKey)
    }

    public func metadataJSONData() -> Data? {
        defaults.data(forKey: Self.metadataDefaultsKey)
    }

    private func allMetadata() -> [ProjectProvider: ProviderCredentialMetadata] {
        guard let data = defaults.data(forKey: Self.metadataDefaultsKey),
              let decoded = try? JSONDecoder().decode([String: ProviderCredentialMetadata].self, from: data)
        else {
            return [:]
        }
        return decoded.reduce(into: [:]) { result, entry in
            guard let provider = ProjectProvider(rawValue: entry.key) else { return }
            result[provider] = entry.value
        }
    }

    private func saveAllMetadata(_ metadata: [ProjectProvider: ProviderCredentialMetadata]) {
        let retained = metadata.filter { _, value in
            value.source != .none || value.oauth != nil
        }
        guard !retained.isEmpty else {
            defaults.removeObject(forKey: Self.metadataDefaultsKey)
            return
        }
        let encoded = Dictionary(uniqueKeysWithValues: retained.map { provider, metadata in
            (provider.rawValue, metadata)
        })
        if let data = try? JSONEncoder().encode(encoded) {
            defaults.set(data, forKey: Self.metadataDefaultsKey)
        }
    }

    private func normalizedSecret(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }
}
