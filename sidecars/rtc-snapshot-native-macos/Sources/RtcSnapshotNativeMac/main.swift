import AgoraRtcKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

struct CaptureConfig: Decodable {
    let appId: String
    let uid: String
    let token: String
    let channel: String
    let expectedPublisher: String?
    let out: String
    let waitMs: Int?
}

final class CaptureState {
    private let lock = NSLock()
    let semaphore = DispatchSemaphore(value: 0)
    var captured = false
    var output: [String: Any]?
    var error: String?
    private var events: [[String: Any]] = []

    func log(_ event: String, _ value: [String: Any] = [:]) {
        lock.lock()
        events.append([
            "event": event,
            "value": value,
            "ts": ISO8601DateFormatter().string(from: Date()),
        ])
        lock.unlock()

        let payload: [String: Any] = ["event": event, "value": value]
        if let data = try? JSONSerialization.data(withJSONObject: payload),
           let line = String(data: data, encoding: .utf8) {
            FileHandle.standardError.write((line + "\n").data(using: .utf8)!)
        }
    }

    func complete(_ output: [String: Any]) {
        lock.lock()
        if captured {
            lock.unlock()
            return
        }
        captured = true
        self.output = output
        lock.unlock()
        semaphore.signal()
    }

    func fail(_ message: String) {
        lock.lock()
        if captured {
            lock.unlock()
            return
        }
        captured = true
        error = message
        lock.unlock()
        semaphore.signal()
    }

    func snapshotEvents() -> [[String: Any]] {
        lock.lock()
        defer { lock.unlock() }
        return events
    }
}

final class CaptureDelegate: NSObject, AgoraRtcEngineDelegate, AgoraVideoFrameDelegate {
    let config: CaptureConfig
    let state: CaptureState

    init(config: CaptureConfig, state: CaptureState) {
        self.config = config
        self.state = state
    }

    func getVideoFormatPreference() -> AgoraVideoFormat {
        .RGBA
    }

    func getVideoFrameProcessMode() -> AgoraVideoFrameProcessMode {
        .readOnly
    }

    func getObservedFramePosition() -> AgoraVideoFramePosition {
        .preRenderer
    }

    func onRenderVideoFrame(
        _ videoFrame: AgoraOutputVideoFrame,
        uid: UInt,
        channelId: String
    ) -> Bool {
        if let expected = config.expectedPublisher, !expected.isEmpty, expected != String(uid) {
            state.log("skip_frame_unexpected_publisher", ["uid": uid, "expected": expected])
            return true
        }
        guard !state.captured else {
            return true
        }

        do {
            let result = try writeJpeg(videoFrame, uid: uid, channelId: channelId)
            state.complete(result)
        } catch {
            state.fail(String(describing: error))
        }
        return true
    }

    func rtcEngine(
        _ engine: AgoraRtcEngineKit,
        didJoinChannel channel: String,
        withUid uid: UInt,
        elapsed: Int
    ) {
        state.log("join_ok", ["channel": channel, "uid": uid, "elapsed": elapsed])
    }

    func rtcEngine(_ engine: AgoraRtcEngineKit, didJoinedOfUid uid: UInt, elapsed: Int) {
        state.log("user_joined", ["uid": uid, "elapsed": elapsed])
    }

    func rtcEngine(
        _ engine: AgoraRtcEngineKit,
        firstRemoteVideoDecodedOfUid uid: UInt,
        size: CGSize,
        elapsed: Int
    ) {
        state.log("first_remote_video_decoded", [
            "uid": uid,
            "width": Int(size.width),
            "height": Int(size.height),
            "elapsed": elapsed,
        ])
    }

    func rtcEngine(
        _ engine: AgoraRtcEngineKit,
        firstRemoteVideoFrameOfUid uid: UInt,
        size: CGSize,
        elapsed: Int
    ) {
        state.log("first_remote_video_frame", [
            "uid": uid,
            "width": Int(size.width),
            "height": Int(size.height),
            "elapsed": elapsed,
        ])
    }

    func rtcEngine(
        _ engine: AgoraRtcEngineKit,
        remoteVideoStateChangedOfUid uid: UInt,
        state videoState: AgoraVideoRemoteState,
        reason: AgoraVideoRemoteReason,
        elapsed: Int
    ) {
        self.state.log("remote_video_state", [
            "uid": uid,
            "state": videoState.rawValue,
            "reason": reason.rawValue,
            "elapsed": elapsed,
        ])
    }

    func rtcEngine(
        _ engine: AgoraRtcEngineKit,
        connectionChangedTo state: AgoraConnectionState,
        reason: AgoraConnectionChangedReason
    ) {
        self.state.log("connection_state", [
            "state": state.rawValue,
            "reason": reason.rawValue,
        ])
    }

    func rtcEngine(_ engine: AgoraRtcEngineKit, didOccurError errorCode: AgoraErrorCode) {
        state.log("error", [
            "code": errorCode.rawValue,
            "description": AgoraRtcEngineKit.getErrorDescription(errorCode.rawValue),
        ])
    }

    private func writeJpeg(
        _ frame: AgoraOutputVideoFrame,
        uid: UInt,
        channelId: String
    ) throws -> [String: Any] {
        guard frame.type == AgoraVideoFormat.RGBA.rawValue else {
            throw captureError("expected RGBA frame, got type \(frame.type)", code: 1)
        }
        guard let source = frame.yBuffer else {
            throw captureError("RGBA frame missing buffer", code: 2)
        }

        let width = Int(frame.width)
        let height = Int(frame.height)
        let bytesPerRow = width * 4
        let byteCount = bytesPerRow * height
        let data = Data(bytes: source, count: byteCount)

        guard let provider = CGDataProvider(data: data as CFData) else {
            throw captureError("failed to create CGDataProvider", code: 3)
        }
        guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else {
            throw captureError("failed to create sRGB color space", code: 4)
        }
        guard let image = CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: bytesPerRow,
            space: colorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ) else {
            throw captureError("failed to create CGImage", code: 5)
        }

        let url = URL(fileURLWithPath: config.out)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        guard let destination = CGImageDestinationCreateWithURL(
            url as CFURL,
            UTType.jpeg.identifier as CFString,
            1,
            nil
        ) else {
            throw captureError("failed to create JPEG destination", code: 6)
        }

        CGImageDestinationAddImage(destination, image, [
            kCGImageDestinationLossyCompressionQuality: 0.92,
        ] as CFDictionary)
        guard CGImageDestinationFinalize(destination) else {
            throw captureError("failed to write JPEG", code: 7)
        }

        let bytes = (try? FileManager.default.attributesOfItem(atPath: config.out)[.size] as? NSNumber)?.intValue ?? 0
        state.log("capture_ok", ["uid": uid, "width": width, "height": height, "bytes": bytes])
        return [
            "ok": true,
            "out": config.out,
            "publisher": String(uid),
            "channel": channelId,
            "width": width,
            "height": height,
            "bytes": bytes,
        ]
    }
}

func captureError(_ message: String, code: Int) -> NSError {
    NSError(
        domain: "EnabotRtcSnapshotNativeMac",
        code: code,
        userInfo: [NSLocalizedDescriptionKey: message]
    )
}

func emit(_ object: [String: Any]) {
    let data = try! JSONSerialization.data(
        withJSONObject: object,
        options: [.prettyPrinted, .sortedKeys]
    )
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write("\n".data(using: .utf8)!)
}

do {
    let input = FileHandle.standardInput.readDataToEndOfFile()
    let config = try JSONDecoder().decode(CaptureConfig.self, from: input)
    guard let uid = UInt(config.uid) else {
        throw captureError("uid is not numeric: \(config.uid)", code: 8)
    }

    let state = CaptureState()
    let delegate = CaptureDelegate(config: config, state: state)
    let engineConfig = AgoraRtcEngineConfig()
    engineConfig.appId = config.appId
    engineConfig.channelProfile = .communication
    engineConfig.areaCode = .global
    let engine = AgoraRtcEngineKit.sharedEngine(with: engineConfig, delegate: delegate)

    let videoDelegateOk = engine.setVideoFrameDelegate(delegate)
    state.log("set_video_frame_delegate", ["ok": videoDelegateOk])
    state.log("enable_video", ["code": engine.enableVideo()])
    state.log("mute_local_audio", ["code": engine.muteLocalAudioStream(true)])
    state.log("mute_local_video", ["code": engine.muteLocalVideoStream(true)])
    state.log("mute_all_remote_video", ["code": engine.muteAllRemoteVideoStreams(false)])

    let options = AgoraRtcChannelMediaOptions()
    options.autoSubscribeAudio = false
    options.autoSubscribeVideo = true
    options.publishCameraTrack = false
    options.publishMicrophoneTrack = false
    options.clientRoleType = .audience

    let joinCode = engine.joinChannel(
        byToken: config.token,
        channelId: config.channel,
        uid: uid,
        mediaOptions: options,
        joinSuccess: nil
    )
    state.log("join_call", ["code": joinCode])
    if joinCode != 0 {
        state.fail("joinChannel returned \(joinCode)")
    }

    let waitMs = config.waitMs ?? 30000
    let timeout = DispatchTime.now() + .milliseconds(waitMs)
    if state.semaphore.wait(timeout: timeout) == .timedOut {
        state.fail("timed out waiting for native RTC frame")
    }

    _ = engine.leaveChannel(nil)
    engine.setVideoFrameDelegate(nil)
    AgoraRtcEngineKit.destroy()

    if let output = state.output {
        var out = output
        out["events"] = state.snapshotEvents()
        emit(out)
    } else {
        emit([
            "ok": false,
            "error": state.error ?? "unknown native capture failure",
            "events": state.snapshotEvents(),
        ])
        exit(1)
    }
} catch {
    emit(["ok": false, "error": String(describing: error)])
    exit(1)
}
