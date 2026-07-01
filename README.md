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

## Notes

- Zero external Rust crates — pure `std`, just orchestrates `yt-dlp` / `ffmpeg` / a small whisper helper.
- Transcription delegates to `faster-whisper` via `scripts/transcribe.py` (kept next to the binary or run from the repo root). Bring your own whisper if you prefer — the CLI only needs `[start] text` lines on stdout.
- `--interval` trades detail for cost: smaller = more frames = more for the model to read.

## License

MIT.
