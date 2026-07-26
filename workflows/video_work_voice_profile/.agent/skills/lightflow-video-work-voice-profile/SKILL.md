---
name: lightflow-video-work-voice-profile
description: This skill should be used when the user asks to "import a voice reference", "create a voice profile", "clone a voice", or "run lightflow.video_work_voice_profile".
version: 0.5.0
---

# Consent-gated Video Work voice profile

Run `lightflow.video_work_voice_profile` only after obtaining explicit,
informed confirmation that the requester has the rights to import the reference
voice. Set `confirm_rights` to `true` only after that confirmation. Provide a
non-empty `speaker_id`, `style_name`, `prompt_text`, and `audio_path` accepted
by the API service sandbox. Preserve transcript content apart from the
workflow's outer-whitespace trimming.

Configure `LIGHTFLOW_VIDEO_WORK_API_URL` and either
`LIGHTFLOW_VIDEO_WORK_API_TOKEN` or `LIGHTFLOW_VIDEO_WORK_API_TOKEN_FILE` only
in the workflow runtime environment. A non-empty token environment value takes
precedence; otherwise the file must be a regular UTF-8 file no larger than 4
KiB with non-empty trimmed contents. Never place or print the URL, token, token
file path, or file contents in workflow inputs, artifacts, source files,
prompts, logs, or command output. Require an HTTPS endpoint unless it targets
loopback.

The workflow calls the service action `add_voice_profile` and returns its
unwrapped profile object. Preserve the returned `voice_profile` as the only
profile evidence. It requires non-empty `id`, `speaker_id`, and `style_name`,
plus a positive `duration_seconds`; otherwise it rejects the service result.
Reject the operation when rights are unconfirmed or any required reference input
is absent; do not request the import from the service.

## CLI Usage

```bash
lfw run lightflow.video_work_voice_profile \
  --input speaker_id='"narrator-01"' \
  --input style_name='"calm-review"' \
  --input prompt_text='"Reference transcript matching the audio sample."' \
  --input audio_path='"media/reference-voice.wav"' \
  --input confirm_rights=true
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_work_voice_profile/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"speaker_id":"narrator-01","style_name":"calm-review","prompt_text":"Reference transcript matching the audio sample.","audio_path":"media/reference-voice.wav","confirm_rights":true}}'
```
