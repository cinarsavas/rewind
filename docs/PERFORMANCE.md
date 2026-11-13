# Performance and low-end PCs

Rewind records continuously, so the cost has to stay low. This page explains where the time goes and how to tune it.

## Quick fix

**Settings, Low-end PC, Apply.** It switches to 720p30, the lightest encoder preset, a 5 Mbps bitrate and a 30 s buffer, and turns off animations, smooth scrolling, toggle effects and thumbnails.

Rewind also does this automatically on first launch when the PC reports four or fewer CPU threads, so an older laptop starts light instead of stuttering at 1080p60.

## Read the numbers

The Replay page shows a **THIS APP** card: CPU % (of the whole machine, like Task Manager), memory, and whether video is encoded on the **GPU** or the **CPU**. Because capture and encoding run inside Rewind's process, that CPU figure *is* the cost of recording.

## What usually causes high CPU

| Cause | What to do |
|---|---|
| **CPU encoding (x264)**: no hardware encoder was found, usually an old or very basic GPU or driver | This is normal on older laptops and still looks good. Apply **Low-end PC** if recording feels heavy; update your GPU driver; avoid 1080p60 on CPU-only encoding. |
| **High resolution or frame rate** | Lower resolution and FPS. 720p30 is roughly a quarter of the work of 1080p60. |
| **Expensive downscaling** | Rewind uses a cheap scaler (bicubic, or bilinear in low-power mode). Native resolution skips scaling entirely. |
| **Busy UI** | Rewind redraws only when something changes and not at all while hidden in the tray. Turn **Smooth animations**, **Smooth scrolling** and **Toggle effects** off for slower laptops. |
| **Voice clipping** | Listening is gated by a noise gate, so it's idle in silence; turn it off if you don't use it. |

## What Rewind already does

- Uses hardware encoders (NVENC / AMF / Quick Sync) whenever present, and falls back to H.264 on the CPU with a calm note instead of an error.
- Captures with DXGI Desktop Duplication by default, which is lighter than Windows Graphics Capture and avoids the Windows 10 yellow recording border. WGC stays available as a fallback in Settings.
- Picks the cheapest x264 preset for the quality level you choose.
- Sleeps its background threads (overlay, tray, hotkeys) instead of polling.
- Caches video thumbnails through Windows and loads them lazily, one at a time.
- Skips drawing when the window is hidden, minimised or idle.

## Reporting a performance problem

Open an issue with: CPU and GPU model, Windows version, your preset or settings, and what the **THIS APP** card shows while recording. That tells us whether it's the encoder, the capture or the UI.
