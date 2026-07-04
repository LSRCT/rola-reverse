// swift-tools-version:5.9

import PackageDescription

let package = Package(
    name: "EnabotRtcSnapshotNativeMac",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "RtcSnapshotNativeMac", targets: ["RtcSnapshotNativeMac"]),
    ],
    targets: [
        .binaryTarget(
            name: "AgoraRtcKit",
            url: "https://download.agora.io/swiftpm/AgoraRtcEngine_macOS/4.6.2/AgoraRtcKit.xcframework.zip",
            checksum: "189aaee1d4cb8f3567dc4251098f77e84c2d2fb4b39067c2a6731aae2174b31a"
        ),
        .binaryTarget(
            name: "Agorafdkaac",
            url: "https://download.agora.io/swiftpm/AgoraRtcEngine_macOS/4.6.2/Agorafdkaac.xcframework.zip",
            checksum: "eb1235366e9b952a71163afeada2fe350f60dca050e866f0b5c1bb0411640ca8"
        ),
        .binaryTarget(
            name: "Agoraffmpeg",
            url: "https://download.agora.io/swiftpm/AgoraRtcEngine_macOS/4.6.2/Agoraffmpeg.xcframework.zip",
            checksum: "ca8fd0f7d008d2398c3616e28dce66b78ab23beb2d1cfcf7d29a5a0d3b7105e3"
        ),
        .binaryTarget(
            name: "AgoraSoundTouch",
            url: "https://download.agora.io/swiftpm/AgoraRtcEngine_macOS/4.6.2/AgoraSoundTouch.xcframework.zip",
            checksum: "fa35927bef8acb16caa774e7c3a2fcc2b27292a03f99a6fa792f28bd90a297f4"
        ),
        .binaryTarget(
            name: "video_dec",
            url: "https://download.agora.io/swiftpm/AgoraRtcEngine_macOS/4.6.2/video_dec.xcframework.zip",
            checksum: "2fabeed4a4dca155cce6ce796e9d561ec9b76c8e273b247260c40301402615b5"
        ),
        .binaryTarget(
            name: "aosl",
            url: "https://download.agora.io/swiftpm/AgoraInfra_macOS/1.3.7/aosl.xcframework.zip",
            checksum: "8d7513a081d0ece099071a283622ec109b5facdabeff9da559cd7f5649a110eb"
        ),
        .executableTarget(
            name: "RtcSnapshotNativeMac",
            dependencies: [
                "AgoraRtcKit",
                "Agorafdkaac",
                "Agoraffmpeg",
                "AgoraSoundTouch",
                "video_dec",
                "aosl",
            ]
        ),
    ]
)
