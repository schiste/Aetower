import XCTest
@testable import AetowerUI

@MainActor
final class ProviderAuthModelsTests: XCTestCase {
    func testManualTokenSetsSourceAndKeepsSecretOutOfMetadataJSON() throws {
        let defaults = makeDefaults()
        let secrets = InMemoryProviderSecretStore()
        let store = ProviderCredentialStore(defaults: defaults, secrets: secrets)

        XCTAssertTrue(store.storeManualToken(" ghp_secret ", for: .github))

        XCTAssertEqual(store.credentialSource(for: .github), .manualToken)
        XCTAssertEqual(store.manualToken(for: .github), "ghp_secret")
        XCTAssertEqual(
            store.resolvedCredential(for: .github),
            ResolvedProviderCredential(source: .manualToken, accessToken: "ghp_secret")
        )
        let metadataJSON = try XCTUnwrap(jsonString(store.metadataJSONData()))
        XCTAssertTrue(metadataJSON.contains(#""github""#))
        XCTAssertTrue(metadataJSON.contains(#""manual_token""#))
        XCTAssertFalse(metadataJSON.contains("ghp_secret"))
    }

    func testOAuthCredentialStoresOnlyMetadataInSettings() throws {
        let defaults = makeDefaults()
        let secrets = InMemoryProviderSecretStore()
        let store = ProviderCredentialStore(defaults: defaults, secrets: secrets)

        let credential = ProviderOAuthCredential(
            accessToken: "oauth_access",
            refreshToken: "oauth_refresh",
            expiresAtMillis: 99_999_999_999_999,
            scopes: ["repo", "repo", "workflow"],
            accountIdentity: "octocat",
            capturedAtMillis: 123
        )

        XCTAssertTrue(store.storeOAuthCredential(credential, for: .github))

        XCTAssertEqual(store.credentialSource(for: .github), .oauth)
        XCTAssertEqual(store.oauthCredential(for: .github), credential)
        XCTAssertEqual(
            store.resolvedCredential(for: .github),
            ResolvedProviderCredential(source: .oauth, accessToken: "oauth_access")
        )
        let metadata = store.metadata(for: .github)
        XCTAssertEqual(metadata.oauth?.scopes, ["repo", "workflow"])
        XCTAssertEqual(metadata.oauth?.accountIdentity, "octocat")
        XCTAssertEqual(metadata.oauth?.capturedAtMillis, 123)
        XCTAssertEqual(metadata.oauth?.hasRefreshToken, true)

        let metadataJSON = try XCTUnwrap(jsonString(store.metadataJSONData()))
        XCTAssertTrue(metadataJSON.contains(#""account_identity":"octocat""#))
        XCTAssertTrue(metadataJSON.contains(#""has_refresh_token":true"#))
        XCTAssertFalse(metadataJSON.contains("oauth_access"))
        XCTAssertFalse(metadataJSON.contains("oauth_refresh"))
    }

    func testExpiredOAuthCredentialDoesNotResolveAccessToken() {
        let defaults = makeDefaults()
        let secrets = InMemoryProviderSecretStore()
        let store = ProviderCredentialStore(defaults: defaults, secrets: secrets)

        let credential = ProviderOAuthCredential(
            accessToken: "expired_access",
            expiresAtMillis: 1,
            capturedAtMillis: 1
        )

        XCTAssertTrue(store.storeOAuthCredential(credential, for: .cloudflare))

        XCTAssertEqual(store.credentialSource(for: .cloudflare), .oauth)
        XCTAssertNil(store.resolvedCredential(for: .cloudflare))
    }

    func testClearingManualTokenFallsBackToExistingOAuthCredential() {
        let defaults = makeDefaults()
        let secrets = InMemoryProviderSecretStore()
        let store = ProviderCredentialStore(defaults: defaults, secrets: secrets)
        let oauth = ProviderOAuthCredential(
            accessToken: "oauth_access",
            capturedAtMillis: 123
        )

        XCTAssertTrue(store.storeOAuthCredential(oauth, for: .github))
        XCTAssertTrue(store.storeManualToken("manual", for: .github))
        XCTAssertEqual(store.credentialSource(for: .github), .manualToken)

        XCTAssertTrue(store.storeManualToken("", for: .github))

        XCTAssertEqual(store.credentialSource(for: .github), .oauth)
        XCTAssertEqual(
            store.resolvedCredential(for: .github),
            ResolvedProviderCredential(source: .oauth, accessToken: "oauth_access")
        )
    }

    func testClearingOAuthCredentialFallsBackToManualToken() {
        let defaults = makeDefaults()
        let secrets = InMemoryProviderSecretStore()
        let store = ProviderCredentialStore(defaults: defaults, secrets: secrets)
        let oauth = ProviderOAuthCredential(
            accessToken: "oauth_access",
            capturedAtMillis: 123
        )

        XCTAssertTrue(store.storeManualToken("manual", for: .github))
        XCTAssertTrue(store.storeOAuthCredential(oauth, for: .github))
        XCTAssertEqual(store.credentialSource(for: .github), .oauth)

        store.clearOAuthCredential(for: .github)

        XCTAssertNil(store.oauthCredential(for: .github))
        XCTAssertEqual(store.credentialSource(for: .github), .manualToken)
        XCTAssertEqual(
            store.resolvedCredential(for: .github),
            ResolvedProviderCredential(source: .manualToken, accessToken: "manual")
        )
    }

    private func makeDefaults() -> UserDefaults {
        let suiteName = "AetowerProviderAuthTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        addTeardownBlock {
            defaults.removePersistentDomain(forName: suiteName)
        }
        return defaults
    }

    private func jsonString(_ data: Data?) -> String? {
        guard let data else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

private final class InMemoryProviderSecretStore: ProviderCredentialSecretStoring {
    private var values: [String: String] = [:]

    func store(_ value: String, account: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            values.removeValue(forKey: account)
        } else {
            values[account] = trimmed
        }
        return true
    }

    func retrieve(account: String) -> String? {
        values[account]
    }

    func delete(account: String) -> Bool {
        values.removeValue(forKey: account)
        return true
    }
}
