# LightFlow Auto Editing

Source-controlled LightFlow workflows and package-owned runners for
automatic video editing.

The project provides executable LightFlow workflow contracts for automatic
editing, Video Work API integration, and XRY batch production:

- `lightflow.video_auto_edit` renders an MP4 only from explicit,
  HMAC-verified VideoScore source clip ranges.
- `lightflow.video_auto_edit_plan` creates a reviewable, versioned edit plan
  only from explicit, HMAC-verified VideoScore source clip ranges.
- `lightflow.video_render_edit` renders an existing edit plan.
- `lightflow.video_cover_image` extracts and composes a cover image.
- `lightflow.video_subtitles` exports an explicit subtitle track and burns one
  selected language into video.
- `lightflow.video_work_subtitles` delegates timestamped subtitle extraction to
  the authenticated Video Work API MCP contract.
- `lightflow.video_work_voice_profile` imports a voice reference only with an
  explicit rights-confirmation input; `lightflow.video_work_speech` queues
  speech from that approved profile.
- `lightflow.xry_batch_produce` runs XRY's canonical, fail-closed production
  stages through LightFlow. It never substitutes a local renderer or bypasses
  XRY's source, subtitle, hook, cover, packaging, or acceptance gates.
- `lightflow.xry_batch_control` is the only Agent-facing control surface for
  bound XRY `progress`, `freeze`, `cleanup`, and `archive` operations.
  Production is available only through `lightflow.xry_batch_produce`.

Automatic editing is intentionally fail-closed: run
`lightflow.video_highlights` with VideoScore first, then pass its `clips` output
unchanged to auto-edit. Each clip carries its matching source path, timestamps,
score, model, editorial reason, `workflow`, and `evidence` tag. Set the same runtime-only
`LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY` (at least 32 non-whitespace bytes) for
both workflows. `lightflow.video_highlights` signs each candidate with
HMAC-SHA256; auto-editing verifies that tag before probing or rendering.

## Requirements

- LightFlow from the parent checkout
- FFmpeg and ffprobe in `PATH`
- For package-owned workflows, LightFlow starts the runner declared in that
  package's `package.metadata.lightflow.runner`; no deployment-owned
  `LIGHTFLOW_COMMAND_RUNNER` dispatcher is required.

The in-tree deployment target is `lightflow-auto-editing-command-dispatcher`.
Build it with `cargo build --release -p lightflow-auto-editing-command-dispatcher`
and configure `LIGHTFLOW_COMMAND_RUNNER` to that resulting regular executable.
It has a closed in-process route table for the current public command workflows;
it does not discover package metadata, resolve paths from input, or start a shell.

Package-owned runners are Rust-native. Media runners invoke only the required
media tools (`ffprobe` and `ffmpeg`) through fixed argument vectors; Python is
neither a runtime dependency nor an Agent-facing implementation path. The
declared package runner is started directly without a shell and is the XRY
execution boundary.

## End-To-End Workflow

From this project root:

```bash
export LFW_PATH="$PWD"

lfw() { cargo run --manifest-path ../../Cargo.toml --bin lfw -- "$@"; }
lfw \
  run lightflow.video_auto_edit \
  --input sources='[
    {"id":"hook","path":"media/hook.mp4","start":0,"end":8,"highlight":{"workflow":"lightflow.video_highlights","source_path":"media/hook.mp4","start_seconds":0,"end_seconds":8,"score":3.7,"model":"TIGER-Lab/VideoScore-v1.1","reason":"Clear full-vehicle opening shot.","evidence":"<generated-by-lightflow.video_highlights>"}},
    {"id":"demo","path":"media/demo.mp4","start":4,"end":20,"highlight":{"workflow":"lightflow.video_highlights","source_path":"media/demo.mp4","start_seconds":4,"end_seconds":20,"score":3.4,"model":"TIGER-Lab/VideoScore-v1.1","reason":"Demonstrates the vehicle feature requested in the brief.","evidence":"<generated-by-lightflow.video_highlights>"}}
  ]' \
  --input brief='"Create a concise product recap with a strong opening."' \
  --input output_path='"output/product-recap.mp4"' \
  --input constraints='{
    "aspect_ratio":"9:16",
    "max_duration_seconds":30,
    "fps":30
  }'
```

String paths are rejected because they cannot carry verified highlight
provenance. The `evidence` values above are placeholders, not usable tags: copy
the actual `clips` output emitted by `lightflow.video_highlights`. Missing durations are discovered with ffprobe. The Rust renderer trims selected
segments, normalizes frame size and frame rate, preserves source audio, and
concatenates the results. It rejects no-audio sources rather than silently
changing their sound, and refuses to replace a source media path.

## Cover Images

`lightflow.video_cover_image` extracts one explicit source timestamp to a PNG
or JPEG artifact. All media and output paths must resolve inside the current
project. Title and image composition are rejected until supplied by their
dedicated composition workflow; the extractor never silently drops them.

```bash
lfw run lightflow.video_cover_image \
  --input source_path='"media/demo.mp4"' \
  --input timestamp_seconds='2.5' \
  --input output_path='"output/cover.png"'
```

## Explicit Multilingual Subtitles

`lightflow.video_subtitles` accepts explicitly supplied BCP-47 subtitle tracks
on the output-video timeline. Cue times are integer `start_ms` / `end_ms`, are
ordered and non-overlapping within each track, and can be exported as UTF-8 SRT
while one selected language is burned into an MP4. The selected font is
mandatory and passed directly to FFmpeg's subtitle renderer.

```bash
lfw run lightflow.video_subtitles \
  --input source_path='"media/demo.mp4"' \
  --input subtitle_tracks='[
    {"language":"zh-CN","cues":[{"start_ms":0,"end_ms":1500,"text":"你好"}]},
    {"language":"en","cues":[{"start_ms":0,"end_ms":1500,"text":"Hello"}]}
  ]' \
  --input selected_language='"zh-CN"' \
  --input font_path='"assets/NotoSansCJK-Regular.ttc"' \
  --input output_path='"output/demo-zh.mp4"' \
  --input srt_output_path='"output/demo-zh.srt"'
```

The bundled runner does not perform ASR, translation, language detection, or
cloud calls. It has no provider dependency or credential configuration.
`transcription_request` and `translation_request` intentionally fail with a
clear unconfigured-provider error; run an external system and pass its verified
results through `subtitle_tracks`.

## Video Work API Nodes

Set the Video Work API endpoint and MCP bearer only in the runtime environment.
Use a non-empty direct token when available, otherwise point to a regular UTF-8
token file no larger than 4 KiB; the client trims file contents before use:

```bash
export LIGHTFLOW_VIDEO_WORK_API_URL='http://127.0.0.1:7860'
export LIGHTFLOW_VIDEO_WORK_API_TOKEN='…'
# Or, when the direct token is unset or empty:
export LIGHTFLOW_VIDEO_WORK_API_TOKEN_FILE='/run/secrets/video-work-mcp-token'
```

The token, token-file path, and token-file contents are never accepted as
workflow input, returned in run evidence, or printed. The subtitle node calls
Video Work API's `extract_video_subtitles`; the voice profile node rejects the
request unless `confirm_rights=true`, and the speech node only accepts an
existing profile ID.

## XRY Batch Production

For one exact frozen XRY task and subject, run the canonical producer only
through LightFlow:

```bash
export LFW_PATH="$PWD"
lfw run lightflow.xry_batch_produce \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"'
```

`task` and `subject` are exact bindings, not paths or shell fragments. The
package-owned runner uses the fixed XRY gateway subsystem; it does not accept
an SSH target, config, remote root, command, or arbitrary path from workflow
input. The gateway returns production evidence only after its bound request and
canonical `PASS` response verify. It cannot author EDL, captions, hooks, cover
controls, cleanup, archive, or publication through this producer.

Before enabling that runner, the deployment owner provisions the invoking
account's fixed `~/.config/lightflow/xry_gateway_identity` and
`~/.config/lightflow/xry_gateway_known_hosts` files. Both files and every
directory through that account's home directory must be owned by the invoking
account and must not be group- or world-writable. The identity must be a
regular non-symlink file readable only by that account. Neither path nor
credential material is a workflow input, run artifact, or agent instruction.

Use the control workflow only for `progress`, `freeze`, `cleanup`, or
`archive`. `cleanup` and `archive` first require a dry run; present the exact
plan SHA-256 from its canonical report to the user, then pass that same value
only after explicit confirmation:

```bash
lfw run lightflow.xry_batch_control \
  --input action='"archive"' \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"' \
  --input subject='"S01"' \
  --input apply=true \
  --input plan_sha256='"<confirmed-plan-sha256>"'
```

Do not invoke XRY shell commands or implementation scripts directly from an
Agent instruction, skill, or operator runbook. If the declared package runner
cannot be started or the gateway is unavailable, stop and report the blocker
rather than adding a transport fallback.

The end-to-end workflow accepts only explicitly supplied source ranges. A
VideoScore highlight must identify the same canonical source path and match the
selected range to within 0.001 seconds; its score must be 1 through 4 and its
workflow must be `lightflow.video_highlights`, its model must be
`TIGER-Lab/VideoScore-v1.1`, and its model, reason, and lowercase-hex HMAC tag
must be valid under `LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY`.

## Planning And Rendering Separately

Plan from metadata:

```bash
lfw run lightflow.video_auto_edit_plan \
  --input clips='[
    {"id":"intro","path":"intro.mp4","start":0,"end":8,"highlight":{"workflow":"lightflow.video_highlights","source_path":"intro.mp4","start_seconds":0,"end_seconds":8,"score":3.7,"model":"TIGER-Lab/VideoScore-v1.1","reason":"Clear opening vehicle shot.","evidence":"<generated-by-lightflow.video_highlights>"}},
    {"id":"body","path":"body.mp4","start":4,"end":20,"highlight":{"workflow":"lightflow.video_highlights","source_path":"body.mp4","start_seconds":4,"end_seconds":20,"score":3.4,"model":"TIGER-Lab/VideoScore-v1.1","reason":"Shows the requested vehicle feature.","evidence":"<generated-by-lightflow.video_highlights>"}}
  ]' \
  --input brief='"Cut a concise launch recap."' \
  --input style='"fast social product edit"' \
  --input constraints='{"aspect_ratio":"9:16","max_duration_seconds":30}'
```

Render from a JSON input object stored in `render-inputs.json`:

```bash
lfw run lightflow.video_render_edit \
  --inputs @render-inputs.json
```

The file contains both workflow inputs:

```json
{
  "edit_plan": {
    "schema": "lightflow.video.edit-plan.v1",
    "timeline": [
      {
        "clip_id": "intro",
        "path": "media/intro.mp4",
        "start": 0,
        "end": 3
      }
    ],
    "output": {
      "aspect_ratio": "16:9",
      "width": 1280,
      "height": 720,
      "fps": 30,
      "max_duration_seconds": 30
    }
  },
  "output_path": "output/rendered.mp4"
}
```

The renderer is invoked only through `lightflow.video_render_edit`; its package
runner is Rust-native and is not an Agent-facing CLI.

## Package Runner Contract

All workflows explicitly declare their runtime requirements. The automatic
editing and XRY workflows use the package-owned runner boundary:

```text
capability: lightflow.runner
engine:     runner.v1
protocol:   lightflow.runner.v1
runner:     package-declared Cargo binary
```

LightFlow starts that declared binary directly, never through a shell. A
versioned JSON request is written to stdin and a bounded JSON response is read
from stdout. The core enforces timeout, output-size, declared-output, artifact,
and replay-fingerprint checks. Package-owned runners validate their exact
workflow identity before execution and fingerprint normalized inputs and source
assets along with their implementation identity. This keeps workflow
serialization stable while retaining project-specific editing logic inside Rust
package runners.

## Verification

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

## Layout

```text
runtime/
  src/                         # Rust-native plan/render/cover/subtitle runtime
workflows/
  auto_edit/                 # automatic editing
  auto_edit_plan/
  cover_image/
  render_edit/
  subtitles/
  video_work_subtitles/      # authenticated Video Work API workflows
  video_work_voice_profile/
  video_work_speech/
  xry_batch_control/         # only Agent-facing XRY controls
  xry_batch_produce/
```
