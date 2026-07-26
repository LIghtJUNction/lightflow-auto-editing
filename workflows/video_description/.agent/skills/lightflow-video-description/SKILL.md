---
name: lightflow-video-description
description: Generate account-specific video descriptions with a configured model and fail-closed fact validation.
version: 0.1.0
---

# Video description

Use `lightflow.video_description` only with the approved edited transcript and
frozen verified facts. The workflow requires `LIGHTFLOW_DESCRIPTION_API_URL`,
`LIGHTFLOW_DESCRIPTION_API_TOKEN`, and `LIGHTFLOW_DESCRIPTION_MODEL` in the
runtime environment. It does not fall back to templates when the model is
unconfigured. Use `account_group=zh` for the Chinese account and
`account_group=overseas` for the Russian overseas account.

The model must return JSON with a title, body, and 3–8 hashtags. Any numeric
claim not present in the frozen facts rejects the result for rework.

## CLI Usage

```bash
lfw run lightflow.video_description \
  --input account_group='"zh"' \
  --input facts='{"vehicle":"2024 pickup","horsepower":400}' \
  --input transcript='"Approved edited transcript text."'
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_description/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"account_group":"zh","facts":{"vehicle":"2024 pickup","horsepower":400},"transcript":"Approved edited transcript text."}}'
```
