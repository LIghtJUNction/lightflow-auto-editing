# XRY progress dashboard

This is a thin LightFlow-facing dashboard for the XRY batch worker. It reads
state from the real `/srv/1.素材`, `/srv/2.预处理`, and `/srv/3.成品` roots and
uses only the stable `lfw-xry` wrapper for progress and manual scheduling.

The dashboard does not render media, generate captions, or bypass a quality
gate. It is deployed as a separate service on port `8318`; Video Work API
remains the separate subtitle service on port `7860`.
