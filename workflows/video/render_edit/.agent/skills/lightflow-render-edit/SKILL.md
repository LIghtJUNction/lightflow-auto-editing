---
name: LightFlow Video Render Edit
description: Use this skill when rendering, testing, or modifying the lightflow.video.render_edit workflow and its ffmpeg-backed demo renderer.
version: 0.1.0
---

# LightFlow Video Render Edit

Use `lightflow.video.render_edit` to represent the render step for an automated
video edit plan. The workflow records the source-controlled contract. The
current executable renderer is `scripts/render_edit.py`, which reads the same
`edit_plan` JSON and writes an MP4 with ffmpeg.

## Contract

- Workflow id: `lightflow.video.render_edit`
- Required input `edit_plan`: JSON edit decision plan.
- Required input `output_path`: destination MP4 path.
- Output `video`: rendered video artifact metadata.
- Output `video_path`: rendered MP4 path.
- Output `render_summary`: readable render summary.

## CLI Usage

```bash
lfw run lightflow.video.render_edit \
  --input edit_plan='@examples/demo/edit_plan.json' \
  --input output_path='"examples/output/auto-edit-demo.mp4"'
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video.render_edit/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"edit_plan":{"timeline":[{"clip_id":"intro","path":"examples/demo/clips/intro.mp4","start":0,"end":2.5,"title":"Hook"}]},"output_path":"examples/output/auto-edit-demo.mp4"}}'
```

## Render Demo

From the repository root:

```bash
python3 scripts/render_edit.py \
  --plan examples/demo/edit_plan.json \
  --output examples/output/auto-edit-demo.mp4
```

Update this skill whenever the edit plan schema, render command, inputs,
outputs, or runtime behavior changes.
