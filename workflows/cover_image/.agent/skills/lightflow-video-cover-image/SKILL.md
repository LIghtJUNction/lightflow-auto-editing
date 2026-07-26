---
name: lightflow-video-cover-image
description: Compose a non-black, account-specific cover from a verified vehicle video frame.
version: 0.2.0
---

# LightFlow Video Cover Image

The published workflow crate owns a Rust-native `runner.v1` binary. FFmpeg and
ffprobe are the only host media tools.

Use `lightflow.video_cover_image` with a source video, an explicit timestamp,
an account group, a title, a font, and a PNG/JPG/JPEG output path. The cover is
always built over the verified source frame: black-background covers are not a
supported output. `zh` produces a warm-orange deal card near the lower frame;
`overseas` produces a blue-cyan export card near the upper frame.

```bash
lfw run lightflow.video_cover_image \
  --input source_path='"media/demo.mp4"' \
  --input timestamp_seconds='2.5' \
  --input account_group='"zh"' \
  --input title='"二手皮卡实拍"' \
  --input font_path='"assets/NotoSansCJK-Regular.ttc"' \
  --input output_path='"output/cover.png"'
```

The same workflow is available through the HTTP API:

```bash
curl -X POST http://127.0.0.1:8787/workflows/lightflow.video_cover_image/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"source_path":"media/demo.mp4","timestamp_seconds":2.5,"account_group":"zh","title":"二手皮卡实拍","font_path":"assets/NotoSansCJK-Regular.ttc","output_path":"output/cover.png"}}'
```
