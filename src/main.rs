// tiktok-reader — turn a video URL into frames + transcript an LLM can read.
//
// A model like Claude Opus can SEE images and READ text, but can't watch a video. This CLI closes that gap:
// it downloads the video (yt-dlp), samples frames every N seconds and pulls the audio (ffmpeg), and
// transcribes the speech (faster-whisper), leaving a folder of JPEGs + a transcript.txt the model reads.
//
// Zero external crates — pure std, orchestrates yt-dlp / ffmpeg / a whisper helper as subprocesses.
// Requires on PATH: yt-dlp, ffmpeg, ffprobe, and (for transcription) python3 with faster-whisper.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

fn have(bin: &str) -> bool {
    // true if the binary is resolvable (it spawned), regardless of exit code —
    // ffmpeg/ffprobe exit non-zero on `--version`, but they exist.
    Command::new(bin).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

struct Opts {
    url: String,
    out: PathBuf,
    interval: u32,
    model: String,
    transcribe: bool,
    digest: bool,
    digest_model: String,
}

fn parse_args() -> Opts {
    let mut a = std::env::args().skip(1);
    let mut url = None;
    let mut out = PathBuf::from("tr_out");
    let mut interval = 5u32;
    let mut model = "base".to_string();
    let mut transcribe = true;
    let mut digest = false;
    let mut digest_model = "qwen2.5:1.5b".to_string();
    while let Some(arg) = a.next() {
        match arg.as_str() {
            "-o" | "--out" => out = PathBuf::from(a.next().unwrap_or_else(|| die("--out needs a value"))),
            "-i" | "--interval" => interval = a.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| die("--interval needs a number (seconds)")),
            "-m" | "--model" => model = a.next().unwrap_or_else(|| die("--model needs a value")),
            "--no-transcribe" => transcribe = false,
            "--digest" => digest = true,
            "--digest-model" => digest_model = a.next().unwrap_or_else(|| die("--digest-model needs a value")),
            "-h" | "--help" => { help(); std::process::exit(0); }
            other => {
                if other.starts_with('-') { die(&format!("unknown flag: {other}")); }
                if url.is_none() { url = Some(other.to_string()); } else { die("only one URL is supported"); }
            }
        }
    }
    Opts { url: url.unwrap_or_else(|| { help(); die("missing <URL>"); }), out, interval, model, transcribe, digest, digest_model }
}

fn help() {
    println!(
        "tiktok-reader — video URL -> frames + transcript for an LLM\n\n\
         USAGE:\n  tiktok-reader <URL> [options]\n\n\
         OPTIONS:\n\
         \x20 -o, --out <DIR>        output directory (default: tr_out)\n\
         \x20 -i, --interval <SEC>   seconds between sampled frames (default: 5)\n\
         \x20 -m, --model <NAME>     whisper model: tiny|base|small|medium (default: base)\n\
         \x20     --no-transcribe    skip audio transcription (frames only)\n\
         \x20     --digest           cascade: a small LOCAL model pre-digests the transcript (grounded,\n\
         \x20                        quotes-only) so a frontier model reads less. Needs ollama.\n\
         \x20     --digest-model <M> ollama model for the digest (default: qwen2.5:1.5b)\n\
         \x20 -h, --help             this help\n\n\
         REQUIRES on PATH: yt-dlp, ffmpeg, ffprobe; python3 + faster-whisper (transcription); ollama (--digest)."
    );
}

fn script_path(name: &str) -> PathBuf {
    // scripts/ next to the binary (../../scripts from target/<profile>/), else relative to cwd.
    std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.join("../../scripts").join(name)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("scripts").join(name))
}

fn run(cmd: &mut Command, what: &str) {
    let status = cmd.status().unwrap_or_else(|e| die(&format!("failed to spawn {what}: {e}")));
    if !status.success() { die(&format!("{what} exited with {status}")); }
}

fn probe_duration(video: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(video).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn main() {
    let o = parse_args();
    for (bin, why) in [("yt-dlp", "download"), ("ffmpeg", "frames/audio"), ("ffprobe", "duration")] {
        if !have(bin) { die(&format!("`{bin}` not found on PATH (needed for {why})")); }
    }
    std::fs::create_dir_all(&o.out).unwrap_or_else(|e| die(&format!("cannot create {}: {e}", o.out.display())));
    let frames = o.out.join("frames");
    std::fs::create_dir_all(&frames).ok();
    let video = o.out.join("video.mp4");

    // 1) download
    eprintln!("[1/4] downloading {} ...", o.url);
    run(Command::new("yt-dlp").args(["--no-playlist", "-o"]).arg(&video).arg(&o.url), "yt-dlp");
    let dur = probe_duration(&video).unwrap_or(0.0);
    eprintln!("      duration {:.1}s", dur);

    // 2) frames every N seconds
    eprintln!("[2/4] sampling frames every {}s ...", o.interval);
    run(Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-i"]).arg(&video)
        .arg("-vf").arg(format!("fps=1/{},scale=640:-1", o.interval))
        .arg("-q:v").arg("3").arg(frames.join("f_%03d.jpg")), "ffmpeg (frames)");
    let n = std::fs::read_dir(&frames).map(|d| d.count()).unwrap_or(0);
    eprintln!("      {n} frames -> {}", frames.display());

    // 3) audio + 4) transcript
    let transcript = o.out.join("transcript.txt");
    if o.transcribe {
        let audio = o.out.join("audio.wav");
        eprintln!("[3/4] extracting audio ...");
        run(Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&video)
            .args(["-ar", "16000", "-ac", "1"]).arg(&audio), "ffmpeg (audio)");
        eprintln!("[4/4] transcribing (whisper {}) ...", o.model);
        let out = Command::new("python3").arg(script_path("transcribe.py")).arg(&audio).arg(&o.model).output()
            .unwrap_or_else(|e| die(&format!("failed to run transcriber: {e}")));
        if !out.status.success() {
            eprintln!("warning: transcription failed:\n{}", String::from_utf8_lossy(&out.stderr));
        } else {
            std::fs::write(&transcript, &out.stdout).ok();
            eprintln!("      transcript -> {}", transcript.display());
        }
    } else {
        eprintln!("[3/4] skipped audio\n[4/4] skipped transcription");
    }

    // optional cascade: small local model pre-digests the transcript (grounded) for a frontier composer
    let digest_path = o.out.join("digest.txt");
    if o.digest && transcript.exists() {
        eprintln!("[5/5] cascade digest (local {}) ...", o.digest_model);
        match Command::new("python3").arg(script_path("digest.py")).arg(&transcript).arg(&o.digest_model).output() {
            Ok(r) if r.status.success() => { std::fs::write(&digest_path, &r.stdout).ok(); eprintln!("      digest -> {}", digest_path.display()); }
            Ok(r) => eprintln!("warning: digest failed:\n{}", String::from_utf8_lossy(&r.stderr)),
            Err(e) => eprintln!("warning: could not run digest: {e}"),
        }
    }

    println!("\n=== DONE ===");
    println!("frames    : {} ({n} @ every {}s)", frames.display(), o.interval);
    if o.transcribe && transcript.exists() { println!("transcript: {}", transcript.display()); }
    if o.digest && digest_path.exists() {
        println!("digest    : {} (grounded extract for the frontier model)", digest_path.display());
        println!("Hand the DIGEST to the frontier model to compose the final answer; keep the transcript as the anchor.");
    } else {
        println!("Point the LLM at the frames/ folder and transcript.txt.");
    }
}
