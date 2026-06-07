# Browser & Format Compatibility

How the QR decoder captures frames across browsers, why each choice was made,
and what video format to use.

## TL;DR

- **Use Chrome / Chromium (or Edge)** for the most reliable, fastest decode.
- **Firefox / Safari**: use the dedicated builds (`vdf-qr-decoder-firefox.html`,
  `vdf-qr-decoder-safari.html`). They capture frames a different way (see below).
- **Record/receive video as `.mp4` (H.264)** — universally decodable, including
  Safari. Phone/screen recordings are already mp4; that is the recommended input.

## Frame-capture strategy (the core cross-browser issue)

The decoder plays the recorded QR video and grabs frames to a `<canvas>` to scan.
How it grabs each frame depends on the browser:

| Browser | API used | Notes |
|---|---|---|
| Chrome / Chromium / Edge | `requestVideoFrameCallback` (rVFC) | Exact per-frame, fastest. Native support. |
| Firefox | `requestAnimationFrame` during playback | Firefox does **not** implement rVFC, so we sample on every repaint while the video plays. |
| Safari | `requestAnimationFrame` during playback | Safari *has* rVFC (15.4+) but its real-time path was unreliable here, so the Safari build forces the rAF path for consistency. |

The build flag `FORCE_RAF` selects the path: `false` in the Chrome build (rVFC),
`true` in the Firefox/Safari builds (rAF).

### Why not "seek + draw" (deterministic stepping)?

An earlier attempt paused the video and seeked to evenly-spaced timestamps. It
worked in Chromium but produced **black frames on Firefox/Safari** — those
engines do not reliably paint a *paused, seeked* video to a canvas. The fix is to
**keep the video playing** and sample painted frames with `requestAnimationFrame`
(guarded by `readyState >= HAVE_CURRENT_DATA` so we never capture an undecoded
frame). Trade-off: rAF sampling runs in real time, so a 10-minute clip takes ~10
minutes to scan — but it is reliable everywhere.

## Other cross-browser hardening

- **Autoplay**: the video is `muted` + `playsinline`; `play()` is called on the
  Start-Scan gesture and retried in the loop if the browser blocks it. Muted
  autoplay is permitted on Firefox/Safari.
- **`playsinline` / `webkit-playsinline`**: required so iOS Safari plays inline
  instead of going fullscreen.
- **Compression codecs**: `zstd` (WebAssembly) for compress, `fzstd` (pure JS)
  for decompress, plus native `gzip` via `CompressionStream` /
  `DecompressionStream` (Chrome 80+, Firefox 113+, Safari 16.4+).
- **Dynamic `import()` of a blob URL** (used to load the embedded codecs):
  Firefox 113+, Safari 15+.

## Offline / airgap

Every page is **fully self-contained** — the QR library, scanner library, and
compression codecs are embedded inline as `<script>` / base64. No CDN, no network
access is required; open the HTML straight from disk (`file://`) and it works.

## Recommended capture workflow

1. Display the QR video with the **encoder** (fullscreen recommended).
2. Record it with a **phone camera** or a **screen recorder** → produces `.mp4`.
   1080p or higher keeps the QR modules crisp; avoid heavy compression.
3. Open the matching **decoder** build for your browser and load the `.mp4`.
4. The file is decompressed and its original checksum verified before download.

## Sources

- requestVideoFrameCallback support — caniuse / MDN
  (Chrome+Safari yes, Firefox no): <https://caniuse.com/mdn-api_htmlvideoelement_requestvideoframecallback>
- Reliable frame extraction needs the `seeked` event **plus** a timeout fallback:
  <https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement/seeked_event>
- iOS inline video / muted-autoplay policy:
  <https://webkit.org/blog/6784/new-video-policies-for-ios/>
- DecompressionStream: <https://developer.mozilla.org/en-US/docs/Web/API/DecompressionStream>
