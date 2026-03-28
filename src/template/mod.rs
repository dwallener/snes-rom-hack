use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const REQUIRED_DIRS: &[&str] = &[
    "assets",
    "assets/sprites",
    "assets/backgrounds",
    "assets/palettes",
    "assets/audio",
    "scenes",
    "entities",
    "scripts",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateKind {
    SingleScreenAction,
    SideScroller,
    VerticalScroller,
    TopDownAction,
    Rpg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameManifest {
    pub name: String,
    pub template: TemplateKind,
    pub title: String,
    pub region: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPlan<'a> {
    project: &'a str,
    template: TemplateKind,
    title: &'a str,
    region: &'a str,
    runtime_status: &'a str,
    planned_outputs: Vec<&'a str>,
}

pub fn run_template_cli(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("init") => run_template_init_cli(&args[1..]),
        Some("validate") => run_template_validate_cli(&args[1..]),
        Some("preview-assets") => run_template_preview_assets_cli(&args[1..]),
        Some("build") => run_template_build_cli(&args[1..]),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected `template init|validate|preview-assets|build ...`",
        )),
    }
}

fn run_template_init_cli(args: &[String]) -> io::Result<()> {
    let mut kind = None::<TemplateKind>;
    let mut out_dir = None::<PathBuf>;
    let mut name = None::<String>;
    let mut title = None::<String>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--kind" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing template kind")
                })?;
                kind = Some(parse_template_kind(value)?);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--name" => {
                index += 1;
                name = args.get(index).cloned();
            }
            "--title" => {
                index += 1;
                title = args.get(index).cloned();
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `template init --kind <kind> --out <dir> [--name <slug>] [--title <title>]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let kind = kind.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--kind <template-kind>` for `template init`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `template init`",
        )
    })?;
    let inferred_name = name.unwrap_or_else(|| {
        out_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("game")
            .to_string()
    });
    let title = title.unwrap_or_else(|| title_case(&inferred_name));

    create_template_project(&out_dir, &GameManifest {
        name: inferred_name,
        template: kind,
        title,
        region: "ntsc".to_string(),
        version: "0.1.0".to_string(),
    })?;

    println!("initialized template project {}", out_dir.display());
    Ok(())
}

fn run_template_validate_cli(args: &[String]) -> io::Result<()> {
    let project = parse_project_arg(args, "template validate --project <dir>")?;
    let manifest = load_manifest(&project)?;
    let issues = validate_project_layout(&project);
    if issues.is_empty() {
        println!(
            "validated template project {} ({})",
            project.display(),
            template_kind_name(manifest.template)
        );
        return Ok(());
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput, issues.join("; ")))
}

fn run_template_preview_assets_cli(args: &[String]) -> io::Result<()> {
    let project = parse_project_arg(args, "template preview-assets --project <dir>")?;
    let manifest = load_manifest(&project)?;
    let summary = format_asset_summary(&project, &manifest)?;
    println!("{summary}");
    Ok(())
}

fn run_template_build_cli(args: &[String]) -> io::Result<()> {
    let mut project = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--project" => {
                index += 1;
                project = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `template build --project <dir> --out <dir>`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let project = project.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--project <dir>` for `template build`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `template build`",
        )
    })?;

    let manifest = load_manifest(&project)?;
    let issues = validate_project_layout(&project);
    if !issues.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, issues.join("; ")));
    }

    fs::create_dir_all(&out_dir)?;
    let plan = BuildPlan {
        project: &manifest.name,
        template: manifest.template,
        title: &manifest.title,
        region: &manifest.region,
        runtime_status: "scaffold-only",
        planned_outputs: vec![
            "engine/runtime.sfc (not implemented yet)",
            "assets/compiled/*.bin (not implemented yet)",
            "build manifest and validation reports",
        ],
    };

    fs::write(
        out_dir.join("build_plan.json"),
        serde_json::to_vec_pretty(&plan).map_err(to_io_error)?,
    )?;
    fs::write(
        out_dir.join("build_notes.txt"),
        format!(
            "Template build scaffold\nproject={}\ntemplate={}\nstatus=scaffold-only\n",
            manifest.name,
            template_kind_name(manifest.template)
        ),
    )?;

    println!(
        "prepared template build scaffold {} -> {}",
        project.display(),
        out_dir.display()
    );
    Ok(())
}

fn parse_project_arg(args: &[String], usage: &str) -> io::Result<PathBuf> {
    if args.len() == 2 && args[0] == "--project" {
        return Ok(PathBuf::from(&args[1]));
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput, usage))
}

fn create_template_project(project: &Path, manifest: &GameManifest) -> io::Result<()> {
    fs::create_dir_all(project)?;
    for dir in REQUIRED_DIRS {
        fs::create_dir_all(project.join(dir))?;
    }

    fs::write(project.join("game.toml"), render_manifest(manifest))?;
    fs::write(project.join("README.md"), render_project_readme(manifest))?;
    fs::write(
        project.join("scenes/room_000.txt"),
        "; single-screen-action scene stub\nid = room_000\nbackground = bg_main\nplayer_spawn = 8,8\n",
    )?;
    fs::write(
        project.join("entities/player.txt"),
        "; player entity stub\nid = player\nkind = player\nsprite = hero_idle\n",
    )?;
    fs::write(
        project.join("scripts/main.txt"),
        "; script stub\non_boot: load_scene room_000\n",
    )?;

    Ok(())
}

fn render_manifest(manifest: &GameManifest) -> String {
    format!(
        "name = \"{}\"\ntemplate = \"{}\"\ntitle = \"{}\"\nregion = \"{}\"\nversion = \"{}\"\n",
        manifest.name,
        template_kind_name(manifest.template),
        manifest.title,
        manifest.region,
        manifest.version
    )
}

fn load_manifest(project: &Path) -> io::Result<GameManifest> {
    let path = project.join("game.toml");
    let text = fs::read_to_string(&path)?;
    parse_manifest(&text)
}

fn parse_manifest(text: &str) -> io::Result<GameManifest> {
    let mut name = None::<String>;
    let mut template = None::<TemplateKind>;
    let mut title = None::<String>;
    let mut region = None::<String>;
    let mut version = None::<String>;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "name" => name = Some(value),
            "template" => template = Some(parse_template_kind(&value)?),
            "title" => title = Some(value),
            "region" => region = Some(value),
            "version" => version = Some(value),
            _ => {}
        }
    }

    Ok(GameManifest {
        name: name.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing name"))?,
        template: template.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing template"))?,
        title: title.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing title"))?,
        region: region.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing region"))?,
        version: version.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing version"))?,
    })
}

fn validate_project_layout(project: &Path) -> Vec<String> {
    let mut issues = Vec::new();
    if !project.join("game.toml").is_file() {
        issues.push("missing game.toml".to_string());
    }
    for dir in REQUIRED_DIRS {
        if !project.join(dir).is_dir() {
            issues.push(format!("missing required directory `{dir}`"));
        }
    }
    issues
}

fn format_asset_summary(project: &Path, manifest: &GameManifest) -> io::Result<String> {
    let mut out = String::new();
    out.push_str("Template Project Asset Summary\n");
    out.push_str(&format!("project: {}\n", project.display()));
    out.push_str(&format!("name: {}\n", manifest.name));
    out.push_str(&format!("template: {}\n", template_kind_name(manifest.template)));
    out.push_str(&format!("title: {}\n", manifest.title));
    out.push('\n');

    for dir in ["assets/sprites", "assets/backgrounds", "assets/palettes", "assets/audio"] {
        let count = fs::read_dir(project.join(dir))?.count();
        out.push_str(&format!("{dir}: {count} entries\n"));
    }
    Ok(out)
}

fn render_project_readme(manifest: &GameManifest) -> String {
    format!(
        "# {}\n\nTemplate: `{}`\n\nThis project was initialized by `template init`.\n\nNext steps:\n\n1. add placeholder assets under `assets/`\n2. define your first room in `scenes/room_000.txt`\n3. run `cargo run -- template validate --project {}`\n4. run `cargo run -- template build --project {} --out build/{}`\n",
        manifest.title,
        template_kind_name(manifest.template),
        manifest.name,
        manifest.name,
        manifest.name
    )
}

fn parse_template_kind(raw: &str) -> io::Result<TemplateKind> {
    match raw {
        "single-screen-action" => Ok(TemplateKind::SingleScreenAction),
        "side-scroller" => Ok(TemplateKind::SideScroller),
        "vertical-scroller" => Ok(TemplateKind::VerticalScroller),
        "top-down-action" => Ok(TemplateKind::TopDownAction),
        "rpg" => Ok(TemplateKind::Rpg),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown template kind `{other}`"),
        )),
    }
}

fn template_kind_name(kind: TemplateKind) -> &'static str {
    match kind {
        TemplateKind::SingleScreenAction => "single-screen-action",
        TemplateKind::SideScroller => "side-scroller",
        TemplateKind::VerticalScroller => "vertical-scroller",
        TemplateKind::TopDownAction => "top-down-action",
        TemplateKind::Rpg => "rpg",
    }
}

fn title_case(raw: &str) -> String {
    raw.split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn to_io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{GameManifest, TemplateKind, parse_manifest, render_manifest, title_case, validate_project_layout};
    use std::fs;

    #[test]
    fn round_trips_manifest() {
        let manifest = GameManifest {
            name: "demo".to_string(),
            template: TemplateKind::SingleScreenAction,
            title: "Demo".to_string(),
            region: "ntsc".to_string(),
            version: "0.1.0".to_string(),
        };
        let parsed = parse_manifest(&render_manifest(&manifest)).expect("parse manifest");
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn title_case_normalizes_slug() {
        assert_eq!(title_case("single_screen-demo"), "Single Screen Demo");
    }

    #[test]
    fn validate_project_layout_reports_missing_dirs() {
        let temp = std::env::temp_dir().join(format!("template-layout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).expect("create temp");
        fs::write(temp.join("game.toml"), "").expect("write manifest");
        let issues = validate_project_layout(&temp);
        assert!(issues.iter().any(|issue| issue.contains("assets")));
        let _ = fs::remove_dir_all(&temp);
    }
}
