import Foundation
import UIKit
import UniformTypeIdentifiers

/// A narrow native export primitive. The host must enforce every reachable
/// editing route and provide a live authorization and selection atomically.
@MainActor
public final class GoyaClipboard {
    public static let maximumUTF8Bytes = 1_048_576

    public enum Result {
        case exported
        case denied
        case noChange
        case tooLarge
        case unsupported
    }

    /// Return a selection only after checking the current principal, focused
    /// target, selection, policy, and protected-content restrictions. A nil
    /// result denies the operation. This callback must not suspend.
    public typealias AuthorizedSelection = () -> String?

    public init() {}

    /// Call synchronously from a host-controlled, explicit copy action only.
    /// The pasteboard write has no acknowledgement of persistence or receipt.
    public func copy(authorizedSelection: AuthorizedSelection) -> Result {
        guard let text = authorizedSelection() else { return .denied }
        guard !text.isEmpty else { return .noChange }
        guard text.utf8.count <= Self.maximumUTF8Bytes else { return .tooLarge }
        UIPasteboard.general.setItems([[UTType.utf8PlainText.identifier: text]], options: [:])
        return .exported
    }

    /// Import requires bounded native decoding and editor revalidation.
    public func paste() -> Result { .unsupported }

    /// Removal requires a reservable editor transaction and observable export.
    public func cut() -> Result { .unsupported }
}
