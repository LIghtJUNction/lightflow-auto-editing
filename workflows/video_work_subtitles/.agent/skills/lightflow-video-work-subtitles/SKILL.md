---
name: lightflow-video-work-subtitles
description: This skill should be used when the user asks to "extract video subtitles", "get timestamped captions", "transcribe a video through Video Work API", or "run lightflow.video_work_subtitles".
version: 0.5.0
---

# Video Work subtitle extraction

Run `lightflow.video_work_subtitles` to request timestamped subtitle extraction
from the authenticated Video Work API MCP service. Provide one non-empty
`video_path` accepted by the API service sandbox. Consume the workflow's
`subtitles` output as the unwrapped service payload: it contains `segments`
(array), `srt` (string), and `words` (array). Do not invent transcript timing.

Configure `LIGHTFLOW_VIDEO_WORK_API_URL` and either
`LIGHTFLOW_VIDEO_WORK_API_TOKEN` or `LIGHTFLOW_VIDEO_WORK_API_TOKEN_FILE` only
in the workflow runtime environment. A non-empty token environment value takes
precedence; otherwise the file must be a regular UTF-8 file no larger than 4
KiB with non-empty trimmed contents. Never place or print the URL, token, token
file path, or file contents in workflow inputs, artifacts, source files,
prompts, logs, or command output. Require an HTTPS endpoint unless it targets
loopback.

Treat service errors, a missing MCP result object, or an invalid subtitle
payload schema as a failed extraction. Do not substitute a local ASR path or
bypass the Video Work API sandbox.

## CLI Usage

```bash
lfw run lightflow.video_work_subtitles \
  --input video_path='"media/interview.mp4"'
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_work_subtitles/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"video_path":"media/interview.mp4"}}'
```
