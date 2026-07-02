import Foundation

/// Canonical JSON decoder configurations. Engine/MCP payloads use snake_case
/// keys; route every such decode through here instead of hand-configuring a
/// JSONDecoder per call site.
enum AetowerJSON {
    /// Fresh decoder for engine/MCP JSON (snake_case keys). Returns a new
    /// instance because JSONDecoder is not documented thread-safe for sharing.
    static func snakeCaseDecoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }
}
