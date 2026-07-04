import Foundation

/// Shared OAuth client-configuration writes for `SettingsStore`.
///
/// Both the Settings integration flow (`SettingsView`) and the Projects
/// connector flow (`ProjectsView`) persist the same GitHub / Cloudflare OAuth
/// client metadata. Keeping the assignment set here means the two call sites
/// stay in sync and neither carries its own copy of the writes.
extension SettingsStore {
    /// Persist GitHub OAuth client configuration (public metadata, not secrets).
    public func applyGitHubOAuthConfiguration(clientID: String, scopesText: String) {
        githubOAuthClientID = clientID
        githubOAuthScopes = scopesText
    }

    /// Persist Cloudflare OAuth client configuration (public metadata, not secrets).
    public func applyCloudflareOAuthConfiguration(
        accountID: String,
        clientID: String,
        scopesText: String,
        redirectURI: String
    ) {
        cloudflareOAuthAccountID = accountID
        cloudflareOAuthClientID = clientID
        cloudflareOAuthScopes = scopesText
        cloudflareOAuthRedirectURI = redirectURI
    }
}
