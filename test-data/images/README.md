# Image test dataset

This directory contains a small, redistributable integration-test corpus for RawWeave.
Every binary is recorded in [`manifest.json`](manifest.json) with its source URL, license,
byte size, SHA-256 digest, and intended coverage.

## Contents

- `raw/`: three CC0 camera files from [raw.pixls.us](https://raw.pixls.us/):
  Nikon NEF (12-bit compressed Bayer), Sony ARW (14-bit compressed Bayer), and Fuji RAF.
- `common/`: two public-domain JPEG photographs from Wikimedia Commons and six small
  [PngSuite](https://github.com/lunapaint/pngsuite) format fixtures covering RGB, RGBA,
  16-bit grayscale+alpha, indexed transparency, interlacing, odd dimensions, and 1×1 input.

The selected RAW files are verified against RawWeave's production `RawloaderDecoder`.
The common images are verified through the Tauri image loader. These integration tests ensure
that the files are useful to this codebase rather than merely valid downloads.

## Validation

From the repository root:

```sh
python3 scripts/validate_image_dataset.py
cargo test -p rawweave-raw --test online_dataset
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml downloaded_common_image_dataset_decodes
```

The checksum validator also rejects unmanifested files. When replacing or adding a file, update
all provenance and checksum fields in `manifest.json` and keep its license compatible with
repository redistribution.

## Scope

This is intentionally a compact smoke/integration corpus, not a comprehensive image-quality
benchmark. The deterministic synthetic RAW corpus remains responsible for controlled clipping,
black-level, orientation, and CFA algorithm assertions.
