---
name: lightflow-video-subtitles
description: Export supplied multilingual timeline subtitles to SRT and burn one selected language into video.
version: 0.2.0
---

# LightFlow Video Subtitles

The package-owned `runner.v1` binary is Rust-native and performs no cloud or
local ASR, translation, or language
detection. Supply already-created subtitle tracks explicitly, then select one
language for SRT export and video burn-in.

```bash
lfw run lightflow.video_subtitles \
  --input source_path='"media/demo.mp4"' \
  --input subtitle_tracks='[{"language":"zh-CN","cues":[{"start_ms":0,"end_ms":1500,"text":"你好"}]},{"language":"en","cues":[{"start_ms":0,"end_ms":1500,"text":"Hello"}]}]' \
  --input selected_language='"zh-CN"' \
  --input font_path='"assets/NotoSansCJK-Regular.ttc"' \
  --input output_path='"output/demo-zh.mp4"' \
  --input srt_output_path='"output/demo-zh.srt"'
```

The same workflow is available through the HTTP API:

```bash
curl -X POST http://127.0.0.1:8787/workflows/lightflow.video_subtitles/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"source_path":"media/demo.mp4","subtitle_tracks":[{"language":"en","cues":[{"start_ms":0,"end_ms":1500,"text":"Hello"}]}],"selected_language":"en","output_path":"output/demo-en.mp4","srt_output_path":"output/demo-en.srt"}}'
```

Passing `transcription_request` or `translation_request` fails clearly because
no provider, dependency, or credential is configured. Use
`lightflow.video_work_subtitles` for authenticated Video Work API subtitle
extraction, then pass the resulting tracks through `subtitle_tracks`.
