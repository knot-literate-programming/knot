"""Record the README demo of Knot's live preview.

A scripted session, like a VHS cassette: the script starts knot-lsp as an
editor would, opens Knot's preview, then types and saves in demo/fixture
(a copy in a temporary directory) with the messages VS Code sends. A
headless browser records a page showing the source next to the preview.

    python demo/record.py [--out docs/book/src/images/demo]

See demo/README.md for the requirements.
"""

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import threading
import time

from playwright.sync_api import sync_playwright

DEMO = pathlib.Path(__file__).resolve().parent
ROOT = DEMO.parent
WIDTH, HEIGHT = 1280, 720
# R and Python messages in English, whatever the system language.
ENGLISH = {"LANGUAGE": "en", "LC_ALL": "en_US.UTF-8"}


class Lsp:
    """A minimal LSP client over knot-lsp's stdio."""

    def __init__(self, command, root, init_options):
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=open(root / "knot-lsp.log", "w"),
            env=dict(os.environ, RUST_LOG="info", **ENGLISH),
        )
        self.lock = threading.Lock()
        self.next_id = 0
        self.responses = {}
        self.compiled = threading.Event()
        threading.Thread(target=self._read, daemon=True).start()
        self.request(
            "initialize",
            {
                "processId": os.getpid(),
                "rootUri": root.as_uri(),
                "workspaceFolders": [{"uri": root.as_uri(), "name": "demo"}],
                "capabilities": {"window": {"showDocument": {"support": True}}},
                "initializationOptions": init_options,
            },
        )
        self.notify("initialized", {})

    def _send(self, message):
        data = json.dumps(message).encode()
        with self.lock:
            self.process.stdin.write(b"Content-Length: %d\r\n\r\n" % len(data) + data)
            self.process.stdin.flush()

    def _read(self):
        stream = self.process.stdout
        while True:
            headers = {}
            while line := stream.readline().strip():
                key, value = line.decode().split(":", 1)
                headers[key.lower()] = value.strip()
            if not headers:
                return
            message = json.loads(stream.read(int(headers["content-length"])))
            if "method" in message and "id" in message:
                self._send({"jsonrpc": "2.0", "id": message["id"], "result": None})
            elif message.get("method") == "knot/compilationComplete":
                self.compiled.set()
            elif "id" in message:
                self.responses[message["id"]] = message

    def request(self, method, params, timeout=180):
        self.next_id += 1
        request_id = self.next_id
        self._send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        deadline = time.time() + timeout
        while request_id not in self.responses:
            if time.time() > deadline:
                raise TimeoutError(method)
            time.sleep(0.02)
        response = self.responses.pop(request_id)
        if "error" in response:
            raise RuntimeError(f"{method}: {response['error']}")
        return response.get("result")

    def notify(self, method, params):
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def close(self):
        self.process.terminate()


class Session:
    """The edited document: the text, the cursor, the page and the server."""

    def __init__(self, lsp, page, path):
        self.lsp, self.page, self.path = lsp, page, path
        self.text = path.read_text()
        self.offset = 0
        self.version = 1
        self.dirty = False
        self.status = ""
        self.first_line = 0
        lsp.notify(
            "textDocument/didOpen",
            {"textDocument": {"uri": path.as_uri(), "languageId": "knot", "version": 1, "text": self.text}},
        )

    def show(self):
        self.page.evaluate(
            "state => window.render(state)",
            {"text": self.text, "offset": self.offset, "dirty": self.dirty,
             "status": self.status, "firstLine": self.first_line},
        )

    def goto(self, anchor, after=True):
        """Put the cursor at `anchor` (after it by default) and sync the preview."""
        start = self.text.index(anchor)
        self.offset = start + len(anchor) if after else start
        self.show()
        line = self.text.count("\n", 0, self.offset)
        self.lsp.request(
            "knot/syncForward",
            {"uri": self.path.as_uri(), "line": line, "character": 0},
        )
        time.sleep(0.4)

    def type(self, text, delay=0.07):
        """Type `text` at the cursor, one keystroke at a time."""
        self.status = "Editing: amber chunks are waiting to run"
        for character in text:
            self.text = self.text[: self.offset] + character + self.text[self.offset :]
            self.offset += 1
            self.dirty = True
            self.version += 1
            self.lsp.notify(
                "textDocument/didChange",
                {"textDocument": {"uri": self.path.as_uri(), "version": self.version},
                 "contentChanges": [{"text": self.text}]},
            )
            self.show()
            time.sleep(delay)

    def delete(self, count, delay=0.09):
        """Delete `count` characters before the cursor."""
        for _ in range(count):
            self.text = self.text[: self.offset - 1] + self.text[self.offset :]
            self.offset -= 1
            self.dirty = True
            self.version += 1
            self.lsp.notify(
                "textDocument/didChange",
                {"textDocument": {"uri": self.path.as_uri(), "version": self.version},
                 "contentChanges": [{"text": self.text}]},
            )
            self.show()
            time.sleep(delay)

    def save(self, status):
        """Save like the editor, then wait until the compilation is complete."""
        time.sleep(0.6)
        self.path.write_text(self.text)
        self.dirty = False
        self.status = "Saved: orange chunks are running"
        self.show()
        self.lsp.compiled.clear()
        self.lsp.notify(
            "textDocument/didSave",
            {"textDocument": {"uri": self.path.as_uri()}, "text": self.text},
        )
        self.lsp.compiled.wait(120)
        time.sleep(0.8)
        self.status = status
        self.show()


def tinymist_from_vscode():
    """The Tinymist bundled with the VS Code extension, as VS Code supplies it."""
    extensions = pathlib.Path.home() / ".vscode" / "extensions"
    for directory in sorted(extensions.glob("myriad-dreamin.tinymist-*"), reverse=True):
        binary = directory / "out" / "tinymist"
        if binary.exists():
            return str(binary)
    return None


def scenario(session):
    """The recorded story: prose, a model on the figure, an error, its fix."""
    session.status = "Up to date"
    session.show()
    time.sleep(2.0)

    # 1. Prose changes appear at once, from the cache.
    session.goto("` minutes")
    session.type(", every 71 minutes or so")
    time.sleep(1.2)
    session.save("Prose only: every result came from the cache")
    time.sleep(1.5)

    # 2. Edit the figure: amber while typing, orange once saved, then the result.
    session.goto('geom_point(colour = "#176b87")')
    session.type(' +\n  geom_smooth(method = "lm", formula = y ~ x)')
    time.sleep(1.0)
    session.save("Up to date: the chunk ran again")
    time.sleep(2.5)

    # 3. A typo: the error is shown where it occurs; later R chunks wait.
    session.goto("eruptions ~ waiti", after=True)
    session.delete(1)
    time.sleep(0.6)
    session.save("An error, in place: later R chunks are suspended, Python still runs")
    time.sleep(3.0)

    # 4. Fix it.
    session.type("i")
    session.save("Fixed: up to date")
    time.sleep(2.5)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", default=str(ROOT / "docs/book/src/images/demo"),
                        help="output path without extension (.mp4 and .gif are written)")
    parser.add_argument("--knot-lsp", default=str(ROOT / "target/release/knot-lsp"))
    parser.add_argument("--knot", default=str(ROOT / "target/release/knot"),
                        help="the knot binary of the same build, to warm the cache")
    arguments = parser.parse_args()

    work = pathlib.Path(tempfile.mkdtemp(prefix="knot-demo-"))
    try:
        shutil.copytree(DEMO / "fixture", work, dirs_exist_ok=True)
        # Warm the cache, so that the recording starts from a compiled document.
        subprocess.run([arguments.knot, "build"], cwd=work, check=True, capture_output=True,
                       env=dict(os.environ, **ENGLISH))
        tinymist = os.environ.get("TINYMIST") or tinymist_from_vscode()
        lsp = Lsp([arguments.knot_lsp], work, {"tinymistPath": tinymist} if tinymist else {})
        main_knot = work / "main.knot"
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch()
                context = browser.new_context(
                    viewport={"width": WIDTH, "height": HEIGHT},
                    record_video_dir=str(work / "video"),
                    record_video_size={"width": WIDTH, "height": HEIGHT},
                )
                page = context.new_page()
                started = time.time()
                session = Session(lsp, page, main_knot)
                result = lsp.request("knot/startPreview", {"uri": main_knot.as_uri()})
                url = f"http://127.0.0.1:{result['staticServerPort']}/?task={result['taskId']}"
                page.goto((DEMO / "stage.html").as_uri() + "?preview=" + url)
                session.show()
                time.sleep(4.0)  # the preview connects and renders
                skip = time.time() - started
                scenario(session)
                video = page.video.path()
                context.close()
                browser.close()
        finally:
            lsp.close()
        out = pathlib.Path(arguments.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-ss", f"{skip:.2f}", "-i", video,
             "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "20", "-movflags", "+faststart",
             f"{out}.mp4"],
            check=True,
        )
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", f"{out}.mp4", "-vf",
             "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];"
             "[b][p]paletteuse=dither=bayer:bayer_scale=4",
             f"{out}.gif"],
            check=True,
        )
        print(f"Wrote {out}.mp4 and {out}.gif")
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
