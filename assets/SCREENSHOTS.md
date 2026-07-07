# Screenshots

The README references `assets/dashboard.png` to showcase the Web Dashboard.
The image is intentionally wrapped in an HTML comment in the README so the
page renders cleanly until a screenshot exists.

## How to capture the dashboard screenshot

1. Build or download the binary, then start the service with the web UI:

   ```bash
   ./tokenizer-cli serve --enable-web
   ```

2. Pull a model so the dashboard has something to show (optional but recommended
   for a populated Encode/Render screenshot):

   ```bash
   ./tokenizer-cli pull Qwen/Qwen2.5-0.5B-Instruct
   ```

3. Open <http://localhost:8080> in your browser, pick the model in the
   top-right selector, type some text on the **Encode** tab, and hit **Encode**.

4. Capture the window (macOS: `Cmd+Shift+4` then space to grab the window, or
   use a browser screenshot extension) and save it here as `dashboard.png`.

5. In `README.md` (and `README.zh-CN.md`), remove the `<!--` and `-->` lines
   wrapping the `<p align="center">…</p>` screenshot block so the image shows up.

Tip: a 2×1 collage of the **Encode** tab (with the token table) and the
**Render** tab (with the rendered prompt) communicates the most value in one
image. Name it `dashboard.png`, or add a second file and reference it too.