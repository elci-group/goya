import Foundation
import WebKit

/// Adapts the generated Lanlink runtime's three POSTs to native iOS networking.
/// The page retains pairing, encryption, replay checks, and media verification.
@MainActor
final class LanTransport: NSObject, WKScriptMessageHandlerWithReply {
    private static let handler = "neptuneLanlink"
    private static let paths = [
        "/lanlink/v0/pair/commit", "/lanlink/v0/pair/reveal", "/lanlink/v0/msg",
    ]
    private static let chunkSize = 256 * 1024
    private static let maxRequestBytes = 64 * 1024
    private static let maxInFlight = 3

    private struct Download {
        let data: Data
        let media: LanMediaLease?
        var offset = 0
    }

    private let endpoint: String
    private let entry: URL
    private let mediaSlot = LanMediaSlot()
    private var pending = 0
    private var downloads: [String: Download] = [:]

    static func install(in configuration: WKWebViewConfiguration, entry: URL) throws {
        let generatedLink = entry.deletingLastPathComponent().appendingPathComponent("lanlink.js")
        let hasLink = FileManager.default.fileExists(atPath: generatedLink.path)
        guard let setting = Bundle.main.object(forInfoDictionaryKey: "NeptuneLanEndpoint") else {
            if hasLink { throw LanTransportError.invalidEndpoint }
            return
        }
        guard hasLink, let endpoint = setting as? String, isLocalEndpoint(endpoint) else {
            throw LanTransportError.invalidEndpoint
        }

        let controller = configuration.userContentController
        let transport = LanTransport(endpoint: endpoint, entry: entry)
        let encoded = try JSONEncoder().encode(endpoint)
        let endpointLiteral = String(decoding: encoded, as: UTF8.self)
        let script = browserAdapter.replacingOccurrences(of: "__NEPTUNE_LAN_ENDPOINT__", with: endpointLiteral)
        controller.addScriptMessageHandler(transport, contentWorld: .page, name: handler)
        controller.addUserScript(WKUserScript(source: script, injectionTime: .atDocumentStart, forMainFrameOnly: true))
    }

    private init(endpoint: String, entry: URL) {
        self.endpoint = endpoint
        self.entry = entry.resolvingSymlinksInPath().standardizedFileURL
    }

    /// Use only a literal local address. No DNS, credentials, redirects, or
    /// user supplied paths are available to this transport.
    private static func isLocalEndpoint(_ endpoint: String) -> Bool {
        guard let components = URLComponents(string: endpoint),
              let host = components.host, let port = components.port,
              (1...65535).contains(port), endpoint == "http://\(host):\(port)" else {
            return false
        }
        let octets = host.split(separator: ".", omittingEmptySubsequences: false)
        let values = octets.compactMap { UInt8($0) }
        guard values.count == 4, host == values.map(String.init).joined(separator: ".") else {
            return false
        }
        return values[0] == 10
            || (values[0] == 172 && (16...31).contains(values[1]))
            || (values[0] == 192 && values[1] == 168)
    }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage,
        replyHandler: @escaping @MainActor @Sendable (Any?, String?) -> Void
    ) {
        guard message.name == Self.handler, message.frameInfo.isMainFrame,
              let document = message.frameInfo.request.url, document.isFileURL,
              document.host == nil || document.host == "", document.query == nil,
              document.resolvingSymlinksInPath().standardizedFileURL.path == entry.path,
              let message = message.body as? [String: Any] else {
            replyHandler(nil, "Lanlink requires the bundled entry document.")
            return
        }
        if let ticket = message["ticket"] as? String {
            guard ticket.utf8.count == 36, let offset = message["offset"] as? Int,
                  var download = downloads.removeValue(forKey: ticket), download.offset == offset else {
                replyHandler(nil, "Lanlink reply is no longer available.")
                return
            }
            let end = min(download.offset + Self.chunkSize, download.data.count)
            let chunk = download.data.subdata(in: download.offset..<end).base64EncodedString()
            download.offset = end
            if end < download.data.count { downloads[ticket] = download }
            replyHandler(chunk, nil)
            return
        }

        guard let path = message["path"] as? String, Self.paths.contains(path),
              let body = message["body"] as? String, body.utf8.count <= Self.maxRequestBytes,
              let url = URL(string: endpoint + path) else {
            replyHandler(nil, "Lanlink request is outside the declared contract.")
            return
        }
        guard pending + downloads.count < Self.maxInFlight else {
            replyHandler(nil, "Too many active Lanlink requests.")
            return
        }
        pending += 1
        LanTransfer.start(
            url: url, body: Data(body.utf8), mediaSlot: path == "/lanlink/v0/msg" ? mediaSlot : nil
        ) { [weak self] result in
            Task { @MainActor in
                guard let self else {
                    replyHandler(nil, "The Lanlink view closed.")
                    return
                }
                self.pending -= 1
                switch result {
                case let .success(response):
                    let ticket = UUID().uuidString
                    if !response.data.isEmpty {
                        self.downloads[ticket] = Download(data: response.data, media: response.media)
                        Task { @MainActor [weak self] in
                            try? await Task.sleep(for: .seconds(60))
                            self?.downloads.removeValue(forKey: ticket)
                        }
                    }
                    replyHandler([
                        "status": response.status,
                        "contentType": response.contentType,
                        "counter": response.counter,
                        "ticket": ticket,
                        "size": response.data.count,
                    ], nil)
                case let .failure(error):
                    replyHandler(nil, error.localizedDescription)
                }
            }
        }
    }

    private static let browserAdapter = """
    (() => {
      "use strict";
      const origin = new URL(__NEPTUNE_LAN_ENDPOINT__).origin;
      const upstreamFetch = window.fetch.bind(window);
      const bridge = window.webkit.messageHandlers.neptuneLanlink;
      window.fetch = async (input, options) => {
        const url = new URL(input instanceof Request ? input.url : input, document.baseURI);
        if (url.origin !== origin) return upstreamFetch(input, options);
        if (url.search || url.hash || url.username || url.password ||
            !options || options.method !== "POST" || typeof options.body !== "string") {
          throw new Error("Unsupported Lanlink request");
        }
        const result = await bridge.postMessage({path: url.pathname, body: options.body});
        if (!Number.isInteger(result.size) || result.size < 0 || result.size > 64 * 1024 * 1024 + 32) {
          throw new Error("Invalid Lanlink response size");
        }
        const bytes = new Uint8Array(result.size);
        for (let offset = 0; offset < bytes.length;) {
          const chunk = atob(await bridge.postMessage({ticket: result.ticket, offset}));
          if (!chunk.length || chunk.length > bytes.length - offset) {
            throw new Error("Invalid Lanlink response chunk");
          }
          for (let i = 0; i < chunk.length; ++i) bytes[offset++] = chunk.charCodeAt(i);
        }
        const headers = new Headers({"Content-Type": result.contentType});
        if (result.counter) headers.set("X-Goya-Ctr", result.counter);
        return new Response(bytes.length ? bytes : null, {status: result.status, headers});
      };
    })();
    """
}

private enum LanTransportError: LocalizedError {
    case invalidEndpoint, invalidResponse, responseTooLarge, mediaBusy, requestFailed

    var errorDescription: String? {
        switch self {
        case .invalidEndpoint: return "The app requires the scene's declared local Lanlink endpoint."
        case .invalidResponse: return "The desktop returned an invalid Lanlink response."
        case .responseTooLarge: return "The Lanlink response exceeds the media limit."
        case .mediaBusy: return "Another Lanlink media reply is still being transferred."
        case .requestFailed: return "Could not reach the paired desktop. Check Local Network access and the desktop address."
        }
    }
}

private struct LanResponse: Sendable {
    let status: Int
    let contentType: String
    let counter: String
    let data: Data
    let media: LanMediaLease?
}

/// The generated runtime reads media slots sequentially. Limit large buffers
/// to one even if the desktop returns media to concurrent control requests.
private final class LanMediaSlot: @unchecked Sendable {
    private let lock = NSLock()
    private var occupied = false

    func acquire() -> LanMediaLease? {
        lock.lock()
        defer { lock.unlock() }
        guard !occupied else { return nil }
        occupied = true
        return LanMediaLease(self)
    }

    fileprivate func release() {
        lock.lock()
        occupied = false
        lock.unlock()
    }
}

private final class LanMediaLease: Sendable {
    private let slot: LanMediaSlot

    init(_ slot: LanMediaSlot) { self.slot = slot }
    deinit { slot.release() }
}

/// URLSession calls this delegate on its serial delegate queue. Bound bytes as
/// they arrive, then hand a single immutable result back to the view.
private final class LanTransfer: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    private let mediaSlot: LanMediaSlot?
    private let complete: @Sendable (Result<LanResponse, LanTransportError>) -> Void
    private var data = Data()
    private var media: LanMediaLease?
    private var received: HTTPURLResponse?
    private var limit = 1024 * 1024
    private var failure: LanTransportError?

    private init(
        mediaSlot: LanMediaSlot?,
        complete: @escaping @Sendable (Result<LanResponse, LanTransportError>) -> Void
    ) {
        self.mediaSlot = mediaSlot
        self.complete = complete
    }

    static func start(
        url: URL, body: Data, mediaSlot: LanMediaSlot?,
        complete: @escaping @Sendable (Result<LanResponse, LanTransportError>) -> Void
    ) {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.urlCredentialStorage = nil
        configuration.urlCache = nil
        configuration.timeoutIntervalForRequest = 30
        configuration.timeoutIntervalForResource = 120
        let delegate = LanTransfer(mediaSlot: mediaSlot, complete: complete)
        let session = URLSession(configuration: configuration, delegate: delegate, delegateQueue: nil)
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        session.dataTask(with: request).resume()
    }

    func urlSession(
        _ session: URLSession, task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
        completionHandler: @escaping @Sendable (URLRequest?) -> Void
    ) {
        failure = .invalidResponse
        completionHandler(nil)
    }

    func urlSession(
        _ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
        completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void
    ) {
        guard let http = response as? HTTPURLResponse,
              (200...599).contains(http.statusCode), !(300...399).contains(http.statusCode) else {
            failure = .invalidResponse
            completionHandler(.cancel)
            return
        }
        if http.mimeType == "application/octet-stream" {
            let counter = http.value(forHTTPHeaderField: "X-Goya-Ctr") ?? ""
            guard let mediaSlot, http.statusCode == 200, counter.utf8.allSatisfy({ (48...57).contains($0) }),
                  let value = UInt64(counter), value > 0, value <= 9_007_199_254_740_991 else {
                failure = .invalidResponse
                completionHandler(.cancel)
                return
            }
            guard let lease = mediaSlot.acquire() else {
                failure = .mediaBusy
                completionHandler(.cancel)
                return
            }
            media = lease
            // A 64 MiB video plus the authenticated encryption tag.
            limit = 64 * 1024 * 1024 + 32
        }
        guard http.expectedContentLength <= Int64(limit) else {
            failure = .responseTooLarge
            completionHandler(.cancel)
            return
        }
        received = http
        completionHandler(.allow)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive bytes: Data) {
        guard failure == nil else { return }
        guard bytes.count <= limit - data.count else {
            failure = .responseTooLarge
            dataTask.cancel()
            return
        }
        data.append(bytes)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        defer {
            data = Data()
            media = nil
            session.finishTasksAndInvalidate()
        }
        if let failure {
            complete(.failure(failure))
        } else if error == nil, let response = received {
            let isMedia = media != nil
            complete(.success(LanResponse(
                status: response.statusCode,
                contentType: isMedia ? "application/octet-stream" : "application/json",
                counter: isMedia ? (response.value(forHTTPHeaderField: "X-Goya-Ctr") ?? "") : "",
                data: data,
                media: media
            )))
        } else {
            complete(.failure(.requestFailed))
        }
    }
}
