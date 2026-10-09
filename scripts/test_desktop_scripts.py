#!/usr/bin/env python3
"""Check desktop build/launch selection without compiling or starting a GUI."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def check():
    source = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="rawweave-desktop-scripts-") as directory:
        root = Path(directory)
        (root / "scripts").mkdir()
        (root / "tools").mkdir()
        for name in ("build-all-in-one.sh", "run-rawweave.sh"):
            shutil.copy2(source / name, root / "scripts" / name)
        cargo = root / "tools/cargo"
        cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == *apps/desktop-gpui/Cargo.toml* && "$*" == *"-p rawweave-gpui"* ]] || exit 17
mkdir -p apps/desktop-gpui/target/release
printf '#!/usr/bin/env bash\\nprintf "gpui:%%s:%%s\\\\n" "${WEBKIT_DISABLE_DMABUF_RENDERER-unset}" "$1"\\n' > apps/desktop-gpui/target/release/rawweave-gpui
''')
        cargo.chmod(0o755)
        pnpm = root / "tools/pnpm"
        pnpm.write_text("#!/usr/bin/env bash\nexit 18\n")
        pnpm.chmod(0o755)
        env = dict(os.environ, PATH=f"{root / 'tools'}:{os.environ['PATH']}")
        env.pop("WEBKIT_DISABLE_DMABUF_RENDERER", None)
        subprocess.run(["bash", "scripts/build-all-in-one.sh"], cwd=root, env=env, check=True)
        assert (root / "bin/rawweave-desktop").is_file()
        result = subprocess.run(["bash", "scripts/run-rawweave.sh", "photo with spaces.raf"], cwd=root, env=env, check=True, capture_output=True, text=True)
        assert result.stdout == "gpui:unset:photo with spaces.raf\n", result.stdout
        env["WEBKIT_DISABLE_DMABUF_RENDERER"] = "caller-value"
        result = subprocess.run(["bash", "scripts/run-rawweave.sh", "photo.cr3"], cwd=root, env=env, check=True, capture_output=True, text=True)
        assert result.stdout == "gpui:caller-value:photo.cr3\n", result.stdout
    print("desktop scripts select GPUI and preserve arguments/environment")


if __name__ == "__main__":
    check()
