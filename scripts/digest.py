#!/usr/bin/env python3
# Cascade step: a SMALL LOCAL model pre-digests the transcript so the frontier model (e.g. Claude Opus)
# reads a compact, GROUNDED extract instead of the whole thing — saving the expensive model's budget.
#
# Grounding guard: the local model must QUOTE, not invent. It extracts only what's in the transcript,
# keeps timestamps, and marks unclear parts — so the final composer stays anchored (the full transcript
# remains on disk for verification). Uses ollama (http://localhost:11434). Bring your own small model.
#
# Usage: digest.py <transcript.txt> [model=qwen2.5:1.5b]
import sys, json, urllib.request

SYS = (
    "You SELECT, you do not interpret. From the TRANSCRIPT, copy the 6-10 most important lines VERBATIM "
    "(exact text + their [timestamp]), the ones that carry the speaker's argument. Do NOT summarize, "
    "translate, rephrase, or add anything of your own — only copy existing lines. Keep their order. "
    "This is a grounded extract: a stronger model will interpret it, so give it the raw quotes, not your opinion.\n"
    "Output only the selected lines, one per line, as: [timestamp] exact text"
)

def main():
    if len(sys.argv) < 2:
        print("usage: digest.py <transcript.txt> [model]", file=sys.stderr); sys.exit(2)
    text = open(sys.argv[1], encoding="utf-8").read()
    model = sys.argv[2] if len(sys.argv) > 2 else "qwen2.5:1.5b"
    body = json.dumps({
        "model": model,
        "messages": [{"role": "system", "content": SYS},
                     {"role": "user", "content": "TRANSCRIPT:\n" + text}],
        "stream": False, "think": False,
        "options": {"temperature": 0, "num_predict": 400},
    }).encode()
    req = urllib.request.Request("http://localhost:11434/api/chat", data=body,
                                 headers={"Content-Type": "application/json"})
    try:
        r = json.loads(urllib.request.urlopen(req, timeout=300).read())
    except Exception as e:
        print(f"ollama not reachable / failed: {e}\n(start it: `ollama serve`; pull a model: `ollama pull {model}`)",
              file=sys.stderr); sys.exit(3)
    print(r["message"]["content"].strip())

if __name__ == "__main__":
    main()
