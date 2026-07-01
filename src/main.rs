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
}

fn parse_args() -> Opts {
    let mut a = std::env::args().skip(1);
    let mut url = None;
    let mut out = PathBuf::from("tr_out");
    let mut interval = 5u32;
    let mut model = "base".to_string();
    let mut transcribe = true;
    while let Some(arg) = a.next() {
        match arg.as_str() {
            "-o" | "--out" => out = PathBuf::from(a.next().unwrap_or_else(|| die("--out needs a value"))),
            "-i" | "--interval" => interval = a.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| die("--interval needs a number (seconds)")),
            "-m" | "--model" => model = a.next().unwrap_or_else(|| die("--model needs a value")),
            "--no-transcribe" => transcribe = false,
            "-h" | "--help" => { help(); std::process::exit(0); }
            other => {
                if other.starts_with('-') { die(&format!("unknown flag: {other}")); }
                if url.is_none() { url = Some(other.to_string()); } else { die("only one URL is supported"); }
            }
        }
    }
    Opts { url: url.unwrap_or_else(|| { help(); die("missing <URL>"); }), out, interval, model, transcribe }
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
         \x20 -h, --help             this help\n\n\
         REQUIRES on PATH: yt-dlp, ffmpeg, ffprobe; python3 + faster-whisper (for transcription)."
    );
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
        let helper = std::env::current_exe().ok()
            .and_then(|p| p.parent().map(|d| d.join("../../scripts/transcribe.py")))
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from("scripts/transcribe.py"));
        let out = Command::new("python3").arg(helper).arg(&audio).arg(&o.model).output()
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

    println!("\n=== DONE ===");
    println!("frames    : {} ({n} @ every {}s)", frames.display(), o.interval);
    if o.transcribe && transcript.exists() { println!("transcript: {}", transcript.display()); }
    println!("Point the LLM at the frames/ folder and transcript.txt.");
}
