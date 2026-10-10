# Image test dataset

This directory contains a small, redistributable integration-test corpus for RawWeave.
Every binary is recorded in [`manifest.json`](manifest.json) with its source URL, license,
byte size, SHA-256 digest, and intended coverage.

## Contents

- `raw/`: seven CC0 camera files from [raw.pixls.us](https://raw.pixls.us/):
  Nikon NEF, Sony ARW, legacy Fuji RAF, X-T4 lossless-compressed X-Trans RAF,
  Canon EOS Kiss F CR2, and EOS-1D X Mark III CR3 RAW/C-RAW.
- `common/`: two public-domain JPEG photographs from Wikimedia Commons and six small
  [PngSuite](https://github.com/lunapaint/pngsuite) format fixtures covering RGB, RGBA,
  16-bit grayscale+alpha, indexed transparency, interlacing, odd dimensions, and 1×1 input.

The selected RAW files are verified against RawWeave's production `RawlerDecoder` (also exposed under the legacy
`RawloaderDecoder` name).
The former Tauri image-loader integration tests were removed with the legacy application.
Common-image checksums remain validated; equivalent GPUI corpus coverage is still needed.

## Validation

From the repository root:

```sh
python3 scripts/validate_image_dataset.py
cargo test -p rawweave-raw --test online_dataset
```

The checksum validator also rejects unmanifested files. When replacing or adding a file, update
all provenance and checksum fields in `manifest.json` and keep its license compatible with
repository redistribution.

## Scope

This is intentionally a compact smoke/integration corpus, not a comprehensive image-quality
benchmark. The deterministic synthetic RAW corpus remains responsible for controlled clipping,
black-level, orientation, and CFA algorithm assertions.
