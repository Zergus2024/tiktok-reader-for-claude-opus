#!/usr/bin/env python3
# Transcription helper for tiktok-reader. Prints "[start] text" lines to stdout.
# Usage: transcribe.py <audio.wav> [model=base]   (needs: pip install faster-whisper)
import sys
def main():
    if len(sys.argv) < 2:
        print("usage: transcribe.py <audio.wav> [model]", file=sys.stderr); sys.exit(2)
    wav = sys.argv[1]; model = sys.argv[2] if len(sys.argv) > 2 else "base"
    try:
        from faster_whisper import WhisperModel
    except ImportError:
        print("faster-whisper not installed: pip install faster-whisper", file=sys.stderr); sys.exit(3)
    m = WhisperModel(model, device="cpu", compute_type="int8", cpu_threads=4)
    segs, info = m.transcribe(wav, beam_size=1)
    print(f"# language: {info.language} (p={info.language_probability:.2f})")
    for s in segs:
        print(f"[{s.start:6.1f}] {s.text.strip()}")
if __name__ == "__main__":
    main()
