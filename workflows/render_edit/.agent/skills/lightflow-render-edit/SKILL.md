---
name: LightFlow Video Render Edit
description: Use this skill when rendering, testing, or modifying the lightflow.video_render_edit workflow and its ffmpeg-backed demo renderer.
version: 0.2.0
---

# LightFlow Video Render Edit

Use `lightflow.video_render_edit` to render a
`lightflow.video.edit-plan.v1` plan. `lfw run` executes the package-owned
runner, which validates media bounds, normalizes video and audio streams,
preserves source audio, supplies silence for clips without audio, and
atomically installs the MP4.
It rejects an `output_path` that resolves to any source media file.

## Runtime

The workflow crate contains a Rust-native `runner.v1` binary. The runner must return all
declared outputs, existing artifact files, and a replay fingerprint.

## Contract

- Workflow id: `lightflow.video_render_edit`
- Required input `edit_plan`: JSON edit decision plan.
- Required input `output_path`: destination MP4 path.
- Output `video`: rendered video artifact metadata.
- Output `video_path`: rendered MP4 path.
- Output `render_summary`: readable render summary.

## CLI Usage

```bash
lfw run lightflow.video_render_edit \
  --input edit_plan='{"schema":"lightflow.video.edit-plan.v1","timeline":[{"clip_id":"intro","path":"media/intro.mp4","start":0,"end":2.5,"title":"Hook"}],"output":{"aspect_ratio":"16:9","fps":30,"width":1280,"height":720,"max_duration_seconds":30}}' \
  --input output_path='"output/auto-edit-demo.mp4"'
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_render_edit/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"edit_plan":{"schema":"lightflow.video.edit-plan.v1","timeline":[{"clip_id":"intro","path":"media/intro.mp4","start":0,"end":2.5,"title":"Hook"}],"output":{"aspect_ratio":"16:9","fps":30,"width":1280,"height":720,"max_duration_seconds":30}},"output_path":"output/auto-edit-demo.mp4"}}'
```

Update this skill whenever the edit plan schema, render command, inputs,
outputs, or runtime behavior changes. Run the crate's Rust tests and Clippy
after any renderer change.
