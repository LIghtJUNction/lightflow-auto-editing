---
name: LightFlow Video Auto Edit Plan
description: Use this skill when creating, running, or modifying the lightflow.video_auto_edit_plan workflow for automated video editing plans.
version: 0.4.0
---

# LightFlow Video Auto Edit Plan

Use `lightflow.video_auto_edit_plan` only after
`lightflow.video_highlights` has returned its approved VideoScore `clips` output.
Pass that output unchanged as `clips`. The
package-owned Rust runner produces a structured `lightflow.video.edit-plan.v1`
from those explicit, HMAC-verified ranges; `lightflow.video_render_edit`
consumes the result.

## Runtime

The workflow crate contains a Rust-native `runner.v1` binary, so installation
does not depend on repository-relative scripts. It does not perform ASR,
semantic selection, silence removal, or scene detection.

## Contract

- Workflow id: `lightflow.video_auto_edit_plan`
- Required input `clips`: JSON array of explicit clip objects with VideoScore
  provenance.
- Required input `brief`: human editing goal, story outline, or script notes.
- Optional input `style`: editing style guidance. Defaults to `clean social edit`.
- Optional input `constraints`: JSON delivery constraints for aspect ratio,
  duration, frame rate, width, and height.
- Output `edit_plan`: JSON edit decision plan.
- Output `summary`: readable planning summary.

## CLI Usage

```bash
# Set this only in the runtime environment. Use the same key for
# lightflow.video_highlights and lightflow.video_auto_edit_plan.
export LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY='at-least-32-non-whitespace-bytes'

lfw run lightflow.video_auto_edit_plan \
  --input clips='[{"id":"intro","path":"intro.mp4","start":0,"end":8,"highlight":{"workflow":"lightflow.video_highlights","source_path":"intro.mp4","start_seconds":0,"end_seconds":8,"score":3.7,"model":"TIGER-Lab/VideoScore-v1.1","reason":"Clear opening vehicle shot.","evidence":"<generated-by-lightflow.video_highlights>"}}]' \
  --input brief='"Cut a concise launch recap with a strong opening hook."' \
  --input style='"fast social product edit"' \
  --input constraints='{"aspect_ratio":"9:16","max_duration_seconds":45}'
```

`<generated-by-lightflow.video_highlights>` is a schema placeholder, not a
usable value. Copy the actual lowercase-hex HMAC-SHA256 tag produced by
`lightflow.video_highlights`; do not forge it.

## API Usage

Start `lfw serve`, then call the workflow through the shared HTTP run contract:

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_auto_edit_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"clips":[{"id":"intro","path":"intro.mp4","start":0,"end":8,"highlight":{"workflow":"lightflow.video_highlights","source_path":"intro.mp4","start_seconds":0,"end_seconds":8,"score":3.7,"model":"TIGER-Lab/VideoScore-v1.1","reason":"Clear opening vehicle shot.","evidence":"<generated-by-lightflow.video_highlights>"}}],"brief":"Cut a concise launch recap with a strong opening hook.","style":"fast social product edit","constraints":{"aspect_ratio":"9:16","max_duration_seconds":45}}}'
```

## Change Notes

When changing inputs, outputs, runtime behavior, or common commands, update this
skill and the runner tests in the same change so agents can safely run and
inspect the workflow.
