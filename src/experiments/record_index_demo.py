#!/usr/bin/env python3
"""Record real CLI indexing in an isolated fixture; dev extras: pillow, pyte."""

import argparse
import codecs
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import shutil
import struct
import subprocess
import tempfile
import termios
import time

import pyte
from PIL import Image, ImageDraw, ImageFont


def fixture(root):
    (root / "pyproject.toml").write_text(
        '[project]\nname = "progress-demo"\nversion = "0.1.0"\n'
        'dependencies = ["fastapi==0.115.0", "pytest==8.3.0"]\n'
    )
    for number in range(20):
        (root / f"routes_{number:02}.py").write_text(
            "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n"
            f'@router.get("/items/{number}")\n'
            f"def get_item_{number}():\n"
            f'    return {{"item": {number}, "ready": True}}\n'
        )
        (root / f"test_routes_{number:02}.py").write_text(
            "import pytest\n\n@pytest.fixture\ndef item():\n"
            f"    return {number}\n\n"
            "def test_item(item):\n    assert item >= 0\n"
        )


def capture(binary, root, columns, rows):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
    env = os.environ.copy()
    for name in ("CI", "NO_COLOR", "REPOGRAMMAR_DIR"):
        env.pop(name, None)
    env.update(TERM="xterm-256color", COLUMNS=str(columns), LANG="en_US.UTF-8", LC_ALL="en_US.UTF-8")
    screen = pyte.Screen(columns, rows)
    stream = pyte.Stream(screen)
    decoder = codecs.getincrementaldecoder("utf-8")("replace")
    frames = []
    transcript = bytearray()
    started = time.monotonic()
    process = subprocess.Popen([str(binary), "init"], cwd=root, env=env,
                               stdin=slave, stdout=slave, stderr=slave)
    os.close(slave)

    def snapshot():
        text = tuple(screen.display)
        if any(line.strip() for line in text) and (not frames or frames[-1][1] != text):
            cells = [[screen.buffer[y][x] for x in range(columns)] for y in range(rows)]
            frames.append((time.monotonic() - started, text, cells))

    try:
        while True:
            if time.monotonic() - started > 120:
                raise TimeoutError("index recording exceeded 120 seconds")
            ready, _, _ = select.select([master], [], [], 0.1)
            if not ready:
                if process.poll() is not None:
                    break
                continue
            try:
                data = os.read(master, 65536)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not data:
                break
            transcript.extend(data)
            # Preserve each actual carriage-return update even when many arrive
            # in one OS read. No invented progress samples or replayed work.
            for character in decoder.decode(data):
                if character in "\r\n":
                    snapshot()
                stream.feed(character)
            snapshot()
        stream.feed(decoder.decode(b"", final=True))
        snapshot()
        return_code = process.wait(timeout=10)
        if return_code:
            raise RuntimeError(f"real init failed with exit code {return_code}")
        return frames, bytes(transcript), time.monotonic() - started
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        os.close(master)
        # This is the only repository whose autosync this recorder may stop.
        stopped = subprocess.run([str(binary), "autosync", "stop", "--project", str(root)],
                                 cwd=root, env=env, capture_output=True, timeout=30)
        if stopped.returncode:
            raise RuntimeError("temporary repository autosync cleanup failed")


PALETTE = {
    "default": "#dbe4ef", "black": "#111a28", "red": "#ff707e",
    "green": "#7de0a6", "brown": "#e7c780", "yellow": "#e7c780",
    "blue": "#83b7ff", "magenta": "#cf9fff", "cyan": "#79dded",
    "white": "#f1f5fa", "brightblack": "#8190a7",
}


def render(cells, font, columns, rows):
    cell_width = round(font.getlength("M"))
    cell_height = 24
    padding = 24
    title_height = 52
    image = Image.new("RGB", (columns * cell_width + padding * 2,
                              rows * cell_height + padding + title_height), "#0d1420")
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, image.width, title_height), fill="#182334")
    for index, color in enumerate(("#ff6c70", "#f3c66b", "#69d6a0")):
        x = padding + index * 22
        draw.ellipse((x, 20, x + 11, 31), fill=color)
    title = "RepoGrammar  /  repogrammar init"
    draw.text(((image.width - font.getlength(title)) / 2, 14), title, font=font, fill="#c8d4e5")
    for y, row in enumerate(cells):
        for x, cell in enumerate(row):
            if cell.data != " ":
                color = PALETTE.get(cell.fg, f"#{cell.fg}" if len(cell.fg) == 6 else "#dbe4ef")
                draw.text((padding + x * cell_width, title_height + y * cell_height),
                          cell.data, font=font, fill=color)
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/repogrammar"))
    parser.add_argument("--output", type=Path, default=Path("docs/assets/index-progress.gif"))
    parser.add_argument("--summary", type=Path, default=Path("docs/demo/index-progress.summary.json"))
    parser.add_argument("--font", type=Path, default=Path("/System/Library/Fonts/Menlo.ttc"))
    args = parser.parse_args()
    binary = args.binary.resolve()
    root = Path(tempfile.mkdtemp(prefix="repogrammar-progress-demo-"))
    try:
        fixture(root)
        frames, transcript, elapsed = capture(binary, root, 96, 12)
    except Exception:
        # Preserve the fixture if cleanup could not establish daemon shutdown.
        raise RuntimeError("recording failed; temporary fixture preserved for inspection") from None
    else:
        shutil.rmtree(root)
    if str(root).encode() in transcript:
        raise RuntimeError("recording contains a private temporary path")
    font = ImageFont.truetype(str(args.font), 16)
    # Keep representative actual screen updates; every distinct stage is kept,
    # and adjacent near-identical progress samples are spaced for readability.
    selected = []
    last_stage = None
    for frame in frames:
        progress = next((line for line in frame[1] if "resync / " in line), "")
        stage = progress.split("[")[0].strip() if progress else None
        if not selected or stage != last_stage or frame[0] - selected[-1][0] >= 0.08:
            selected.append(frame)
            last_stage = stage
    if selected[-1] is not frames[-1]:
        selected.append(frames[-1])
    images = [render(frame[2], font, 96, 12) for frame in selected]
    durations = [max(160, min(700, round((selected[i + 1][0] - frame[0]) * 1000)))
                 if i + 1 < len(selected) else 3000 for i, frame in enumerate(selected)]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    images[0].save(args.output, save_all=True, append_images=images[1:],
                   duration=durations, loop=0, optimize=True, disposal=2)
    args.summary.parent.mkdir(parents=True, exist_ok=True)
    args.summary.write_text(json.dumps({
        "command": "repogrammar init", "working_directory": "isolated temporary fixture",
        "fixture": {"python_files": 40, "config_files": 1,
                    "description": "20 FastAPI routers and 20 pytest modules; no dependencies installed or fixture code executed"},
        "exit_code": 0, "elapsed_seconds": round(elapsed, 3),
        "terminal": {"columns": 96, "rows": 12, "TERM": "xterm-256color"},
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "captured_output_sha256": hashlib.sha256(transcript).hexdigest(),
        "captured_screen_updates": len(frames), "gif_frames": len(images),
        "playback_seconds": sum(durations) / 1000,
        "playback_note": "Actual PTY output; representative frames retained, each held at least 160ms and at most 700ms, final frame held 3s. Playback is not a runtime benchmark. Title bar is presentation chrome, not CLI output.",
        "cleanup": "Stopped only the temporary fixture autosync; temporary directory removed",
        "reproduce": "python src/experiments/record_index_demo.py --binary target/debug/repogrammar (requires pillow and pyte; use --font on Linux)",
    }, indent=2) + "\n")
    print(json.dumps({"frames": len(images), "elapsed_seconds": elapsed, "output": str(args.output)}))


if __name__ == "__main__":
    main()
