---
name: lightflow-video-highlights
description: This skill should be used when the user asks to identify long-video highlights with VideoScore for LightFlow automatic editing.
version: 0.3.0
---

# LightFlow VideoScore Highlights

Use `lightflow.video_highlights` before `lightflow.video_auto_edit_plan` when
working from a long source. Set `LIGHTFLOW_VIDEOSCORE_API_URL` and the
runtime-only `LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY` before running it. Require the
evidence key to contain at least 32 non-whitespace bytes. This workflow fixes
the model to `TIGER-Lab/VideoScore-v1.1`; do not override it. The workflow is
Rust-native and sends no credentials in workflow inputs or artifacts.

Mount the source media read-only at the same path in the VideoScore service.
The service must implement `POST /v1/highlights`: accept `model`,
`source_path`, `brief`, `window_seconds`, and `stride_seconds`; return
`{"segments":[{"start_seconds":number,"end_seconds":number,
"aggregate_score":number,"dimensions":object,"reason":string}]}`. Scores
are on the VideoScore 1–4 scale.

Each returned candidate includes `workflow`, `source_path`, timestamps, score,
the fixed model, reason, and a lowercase-hex HMAC-SHA256 `evidence` tag. The
`clips` output wraps these signed candidates directly as `{id,path,start,end,
highlight}`. Pass `clips` unchanged to `lightflow.video_auto_edit_plan` as
`clips`, or to `lightflow.video_auto_edit` as `sources`, using the same evidence
key. LightFlow rejects altered paths, timestamps, scores, models, reasons, or
tags before media probing.

```bash
lfw run lightflow.video_highlights \
  --input source_path='"media/long-interview.mp4"' \
  --input brief='"Find clear, energetic vehicle walk-around moments with the full car visible."' \
  --input window_seconds=12 \
  --input stride_seconds=6 \
  --input max_highlights=8 \
  --input minimum_score=2.8
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_highlights/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"source_path":"media/long-interview.mp4","brief":"Find clear, energetic vehicle walk-around moments with the full car visible.","window_seconds":12,"stride_seconds":6,"max_highlights":8,"minimum_score":2.8}}'
```
