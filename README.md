# tiktok-reader — for Claude Opus

Turn a TikTok (or any [yt-dlp](https://github.com/yt-dlp/yt-dlp)-supported URL) into **frames + a transcript** an LLM can actually read.

A model like Claude Opus can *see* images and *read* text, but it can't watch a video. This tiny Rust CLI closes that gap:

1. **downloads** the video (`yt-dlp`),
2. **samples frames** every *N* seconds and **extracts the audio** (`ffmpeg`),
3. **transcribes** the speech ([faster-whisper](https://github.com/SYSTRAN/faster-whisper)),

…leaving a folder of JPEGs + a `transcript.txt` you point the model at. It works great on talking-head clips (burned-in captions in the frames, speech in the transcript) and screen-recordings alike.

## Why

Ask an LLM "what's in this video?" and it can't — video isn't in its input modality. But *frames* are images it can see, and a *transcript* is text it can read. `tiktok-reader` does the boring conversion so the model does the understanding.

## Install

Requires on `PATH`:

```bash
# yt-dlp + ffmpeg
pipx install yt-dlp            # or: pip install yt-dlp
sudo apt install ffmpeg        # ffmpeg + ffprobe

# transcription (optional; skip with --no-transcribe)
pip install faster-whisper
```

Build:

```bash
cargo build --release
# binary at target/release/tiktok-reader
```

## Usage

```bash
tiktok-reader <URL> [options]

  -o, --out <DIR>        output directory (default: tr_out)
  -i, --interval <SEC>   seconds between sampled frames (default: 5)
  -m, --model <NAME>     whisper model: tiny|base|small|medium (default: base)
      --no-transcribe    frames only, skip audio
  -h, --help
```

Example:

```bash
tiktok-reader "https://www.tiktok.com/@user/video/123..." -o clip -i 5 -m small
# -> clip/frames/f_001.jpg ...   clip/transcript.txt
```

Then hand `clip/frames/` and `clip/transcript.txt` to the model.

## Cascade: let a small local model do the reading (`--digest`)

You don't need a frontier model to *extract* — only to *compose the final answer*. So chain them:

```
video ──▶ frames + transcript ──▶ [small LOCAL model] ──▶ grounded digest ──▶ [frontier model] ──▶ answer
                                    cheap, on your box                         reads the digest, not the raw
```

```bash
tiktok-reader "<URL>" -o clip --digest --digest-model qwen2.5:1.5b
# -> clip/digest.txt   (compact, quote-based extract)
```

A small local model (via [ollama](https://ollama.com)) reads the whole transcript and emits a compact digest — topic, key points as `[timestamp] "verbatim quote"`, and the speaker's main claim. The **frontier model then reads the digest, not the raw transcript + 18 frames**, so it spends a fraction of the budget.

**Grounding guard.** The digest step is instructed to *quote, not invent*: use only the transcript, keep timestamps, mark garbled lines `[unclear]`. And the full `transcript.txt` stays on disk as the **anchor** — if the composer needs to verify or the digest looks thin, the source is right there. That's the difference between a cheap cascade and a game of telephone: the small model *retrieves*, it doesn't *hallucinate a summary*.

## Caveats — not every model can compress a video

The `--digest` cascade is only as good as the small model doing the compressing. **A model too weak for the content will distort it**, and then the frontier model composes a confident answer on a wrong premise.

Real example from this repo's first test: a 90-second clip where the speaker argues *against* relying on a single prompt (he builds a multi-bot pipeline instead). A 1.5B model asked to "summarize the argument" returned the **opposite** — it reported the speaker's thesis as *"a good prompt leads to the correct behavior."* Backwards. That is exactly the failure this tool is built to avoid, so:

- The digest prompt was changed from *summarize* to **select verbatim lines** — retrieval, not interpretation. A small model can reliably *copy* the key lines even when it can't *reason* about them.
- The full `transcript.txt` always stays on disk as the **anchor**. If a digest looks thin or off, the composer reads the source.
- If in doubt, **skip `--digest`** and hand the frontier model the raw `transcript.txt` + `frames/` directly. No compression, no distortion.

Rules of thumb: use a capable-enough digest model (a 3B–7B handles nuance far better than a 1.5B), watch out for cross-lingual/accented audio (transcription noise compounds), and treat the cascade as an *optimization*, not a *source of truth*.

**But even at its worst, this beats nothing.** An LLM can't watch a video at all. With this tool it gets frames it can see and a transcript it can read — with or without the cascade. The digest is a bonus that saves budget; the frames + transcript are the floor, and the floor is already a capability the model didn't have.

## Notes

- Zero external Rust crates — pure `std`, just orchestrates `yt-dlp` / `ffmpeg` / a small whisper helper.
- Transcription delegates to `faster-whisper` via `scripts/transcribe.py` (kept next to the binary or run from the repo root). Bring your own whisper if you prefer — the CLI only needs `[start] text` lines on stdout.
- `--interval` trades detail for cost: smaller = more frames = more for the model to read.

## License

MIT.
