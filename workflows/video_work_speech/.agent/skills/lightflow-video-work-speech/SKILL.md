---
name: lightflow-video-work-speech
description: This skill should be used when the user asks to "generate speech from a voice profile", "synthesize speech", "use a cloned voice", or "run lightflow.video_work_speech".
version: 0.5.0
---

# Video Work speech generation

Run `lightflow.video_work_speech` only with the `profile_id` returned by a
completed, consent-gated `lightflow.video_work_voice_profile` run. Treat that
lineage as an operating policy: this workflow validates only a non-empty ID.
Provide non-empty `speaker_id`, `profile_id`, and `target_text`. Keep `speed`
within the workflow's accepted 0.75 through 1.25 range.

Configure `LIGHTFLOW_VIDEO_WORK_API_URL` and either
`LIGHTFLOW_VIDEO_WORK_API_TOKEN` or `LIGHTFLOW_VIDEO_WORK_API_TOKEN_FILE` only
in the workflow runtime environment. A non-empty token environment value takes
precedence; otherwise the file must be a regular UTF-8 file no larger than 4
KiB with non-empty trimmed contents. Never place or print the URL, token, token
file path, or file contents in workflow inputs, artifacts, source files,
prompts, logs, or command output. Require an HTTPS endpoint unless it targets
loopback.

The service action `generate_speech` returns a completed generation, not a
queued receipt. The workflow accepts it only when `id`, `audio_url`, and
`audio_path` are non-empty, `status` is `complete`, and `audio` is an object.
Use that validated `generation` JSON as the audio-production evidence.

## CLI Usage

```bash
lfw run lightflow.video_work_speech \
  --input speaker_id='"narrator-01"' \
  --input profile_id='"profile-abc123"' \
  --input target_text='"Text to synthesize with the approved voice profile."'
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_work_speech/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"speaker_id":"narrator-01","profile_id":"profile-abc123","target_text":"Text to synthesize with the approved voice profile."}}'
```
