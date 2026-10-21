use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use craft_font_index as cfi;
use serde_json::Value;

const USAGE: &str = "usage:
  craft-font-index google --repo <google/fonts checkout> [--commit <sha>] --out <index.json>
  craft-font-index noto --mirror <fetch-noto.sh mirror> --repo <google/fonts checkout>
                        --google-index <google index.json> --out <index.json>
  craft-font-index sources --out <sources.json> <index.json>...
The index commands also write <index.json>.gz. --commit defaults to the checkout's HEAD.
Index paths in sources.json are relative to its directory.";

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

#[derive(Default)]
struct Args {
    repo: Option<PathBuf>,
    out: Option<PathBuf>,
    commit: Option<String>,
    mirror: Option<PathBuf>,
    google_index: Option<PathBuf>,
    rest: Vec<PathBuf>,
}

fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut val = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--repo" => a.repo = Some(PathBuf::from(val("--repo")?)),
            "--out" => a.out = Some(PathBuf::from(val("--out")?)),
            "--commit" => a.commit = Some(val("--commit")?),
            "--mirror" => a.mirror = Some(PathBuf::from(val("--mirror")?)),
            "--google-index" => a.google_index = Some(PathBuf::from(val("--google-index")?)),
            "-h" | "--help" => return Ok(None),
            other if other.starts_with("--") => {
                return Err(format!("unknown argument `{other}`\n{USAGE}"));
            }
            other => a.rest.push(PathBuf::from(other)),
        }
    }
    Ok(Some(a))
}

fn write_index(index: &Value, out: &Path, warnings: usize) -> Result<(), String> {
    let fams = index["families"].as_array().map_or(&[][..], Vec::as_slice);
    let files: usize = fams
        .iter()
        .map(|f| f["files"].as_array().map_or(0, Vec::len))
        .sum();
    let text = cfi::to_json(index);
    let gz = cfi::gzip(text.as_bytes());
    std::fs::write(out, &text).map_err(|e| format!("write {}: {e}", out.display()))?;
    let mut gz_path = out.to_path_buf().into_os_string();
    gz_path.push(".gz");
    std::fs::write(&gz_path, &gz)
        .map_err(|e| format!("write {}: {e}", Path::new(&gz_path).display()))?;
    println!(
        "{} families, {} files, {} bytes ({} gzipped), {} warning(s) -> {}",
        fams.len(),
        files,
        text.len(),
        gz.len(),
        warnings,
        out.display()
    );
    Ok(())
}

fn print_warnings(warnings: &[String]) {
    for w in warnings {
        eprintln!("warning: {w}");
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let Some((cmd, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    let Some(a) = parse(rest)? else {
        println!("{USAGE}");
        return Ok(());
    };
    let out = a.out.ok_or(USAGE)?;
    let mut warnings = Vec::new();
    match cmd.as_str() {
        "google" => {
            let repo = a.repo.ok_or(USAGE)?;
            let commit = match a.commit {
                Some(c) => c,
                None => git_head(&repo)?,
            };
            let index = cfi::build_index(&repo, &commit, None, &mut warnings)?;
            print_warnings(&warnings);
            write_index(&index, &out, warnings.len())
        }
        "noto" => {
            let (mirror, repo, gi) = (
                a.mirror.ok_or(USAGE)?,
                a.repo.ok_or(USAGE)?,
                a.google_index.ok_or(USAGE)?,
            );
            let google: Value = serde_json::from_slice(
                &std::fs::read(&gi).map_err(|e| format!("{}: {e}", gi.display()))?,
            )
            .map_err(|e| format!("{}: {e}", gi.display()))?;
            let lang = cfi::lang::LangData::load(&repo, &mut warnings)?;
            let (index, counts) =
                cfi::noto::build_index(&mirror, &lang, &google, None, &mut warnings)?;
            print_warnings(&warnings);
            println!("{counts:?}");
            write_index(&index, &out, warnings.len())
        }
        "sources" => {
            let base = out.parent().unwrap_or(Path::new("")).to_path_buf();
            let mut loaded = Vec::new();
            for p in &a.rest {
                let rel = p
                    .strip_prefix(&base)
                    .map_err(|_| format!("{} is not under {}", p.display(), base.display()))?
                    .to_string_lossy()
                    .to_string();
                let json = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                let mut gz_path = p.clone().into_os_string();
                gz_path.push(".gz");
                let gz = std::fs::read(&gz_path)
                    .map_err(|e| format!("{}: {e}", Path::new(&gz_path).display()))?;
                loaded.push((rel, json, gz));
            }
            let entries: Vec<cfi::sources::Entry> = loaded
                .iter()
                .map(|(path, json, gz)| cfi::sources::Entry { path, json, gz })
                .collect();
            let text = cfi::sources::to_json(&cfi::sources::build(&entries)?);
            std::fs::write(&out, text).map_err(|e| format!("write {}: {e}", out.display()))
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("craft-font-index: {e}");
            ExitCode::FAILURE
        }
    }
}
