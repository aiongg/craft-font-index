use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str =
    "usage: google-fonts-index --repo <google/fonts checkout> [--commit <sha>] --out <index.json>
  Writes <index.json> and <index.json>.gz. --commit defaults to the checkout's HEAD.";

fn git_head(repo: &Path) -> Result<String, String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| format!("--commit not given and git failed: {e}"))?;
    if !o.status.success() {
        return Err("--commit not given and `git rev-parse HEAD` failed in the checkout".into());
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn run(args: &[String]) -> Result<(), String> {
    let (mut repo, mut out, mut commit) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match a.as_str() {
            "--repo" => repo = Some(PathBuf::from(val("--repo")?)),
            "--out" => out = Some(PathBuf::from(val("--out")?)),
            "--commit" => commit = Some(val("--commit")?),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    let repo = repo.ok_or(USAGE)?;
    let out = out.ok_or(USAGE)?;
    let commit = match commit {
        Some(c) => c,
        None => git_head(&repo)?,
    };
    let mut warnings = Vec::new();
    let index = google_fonts_index::build_index(&repo, &commit, None, &mut warnings)?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    let fams = index["families"].as_array().map_or(&[][..], Vec::as_slice);
    let files: usize = fams
        .iter()
        .map(|f| f["files"].as_array().map_or(0, Vec::len))
        .sum();
    let text = google_fonts_index::to_json(&index);
    let gz = google_fonts_index::gzip(text.as_bytes());
    std::fs::write(&out, &text).map_err(|e| format!("write {}: {e}", out.display()))?;
    let mut gz_path = out.clone().into_os_string();
    gz_path.push(".gz");
    std::fs::write(&gz_path, &gz)
        .map_err(|e| format!("write {}: {e}", Path::new(&gz_path).display()))?;
    println!(
        "{} families, {} files, {} bytes ({} gzipped), {} warning(s) -> {}",
        fams.len(),
        files,
        text.len(),
        gz.len(),
        warnings.len(),
        out.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("google-fonts-index: {e}");
            ExitCode::FAILURE
        }
    }
}
