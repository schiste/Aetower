import Foundation
import Security

/// Minimal wrapper over the macOS Keychain for storing secrets that must never
/// touch UserDefaults. Values live as generic passwords under a fixed service,
/// keyed by account.
public enum KeychainHelper {
    private static let service = "com.aeptus.aetower"

    /// Keychain account under which the VirusTotal API key is stored.
    public static let binaryReputationAccount = "binaryReputation"
    /// Keychain account under which the optional GitHub project-provider token is stored.
    public static let githubProviderTokenAccount = "githubProviderToken"
    /// Keychain account under which the optional Cloudflare project-provider token is stored.
    public static let cloudflareProviderTokenAccount = "cloudflareProviderToken"

    /// Store (or overwrite) a secret for `account`. Passing an empty string
    /// deletes the item. Returns true on success.
    @discardableResult
    public static func store(_ value: String, account: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return delete(account: account)
        }
        guard let data = trimmed.data(using: .utf8) else { return false }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        // Replace any existing item so we never accumulate duplicates.
        SecItemDelete(query as CFDictionary)
        var insert = query
        insert[kSecValueData as String] = data
        // ThisDeviceOnly keeps provider tokens out of unencrypted local
        // backups and device-migration transfers; a leaked backup must not
        // carry the user's GitHub/Cloudflare credentials off the machine.
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(insert as CFDictionary, nil) == errSecSuccess
    }

    /// Retrieve the secret for `account`, or nil if absent.
    public static func retrieve(account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess,
            let data = result as? Data,
            let value = String(data: data, encoding: .utf8)
        else {
            return nil
        }
        return value
    }

    /// Delete the secret for `account`. Returns true if removed or already absent.
    @discardableResult
    public static func delete(account: String) -> Bool {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(query as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }

    /// Whether a non-empty secret exists for `account`.
    public static func exists(account: String) -> Bool {
        retrieve(account: account)?.isEmpty == false
    }
}
