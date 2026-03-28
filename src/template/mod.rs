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

const REQUIRED_FILES: &[&str] = &["game.toml", "memory.toml", "contracts.toml", "README.md"];

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
struct MemoryModel<'a> {
    template: TemplateKind,
    engine_region: BankRegion<'a>,
    content_region: BankRegion<'a>,
    work_ram: BankRegion<'a>,
    vram: BankRegion<'a>,
    cgram: BankRegion<'a>,
    oam: BankRegion<'a>,
    dma_budget: DmaBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BankRegion<'a> {
    name: &'a str,
    start: &'a str,
    end: &'a str,
    purpose: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DmaBudget {
    vram_bytes_per_frame: u16,
    cgram_bytes_per_frame: u16,
    oam_bytes_per_frame: u16,
    max_transfers_per_frame: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ContentContracts<'a> {
    template: TemplateKind,
    scenes: SceneContract<'a>,
    sprites: SpriteContract<'a>,
    entities: EntityContract<'a>,
    audio: AudioContract<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SceneContract<'a> {
    scene_format: &'a str,
    max_rooms: u16,
    tilemap_size: &'a str,
    background_layers: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SpriteContract<'a> {
    sprite_format: &'a str,
    palette_format: &'a str,
    max_sprite_tiles_per_room: u16,
    max_metasprites_per_room: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct EntityContract<'a> {
    entity_format: &'a str,
    max_entities_per_room: u16,
    player_slots: u8,
    script_hook_format: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AudioContract<'a> {
    music_format: &'a str,
    sfx_format: &'a str,
    max_music_tracks: u8,
    max_sfx_ids: u8,
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
            "memory layout and content contract reports",
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
            "Template build scaffold\nproject={}\ntemplate={}\nstatus=scaffold-only\ninputs=game.toml,memory.toml,contracts.toml\n",
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
    fs::write(
        project.join("memory.toml"),
        render_memory_model(&default_memory_model(manifest.template)),
    )?;
    fs::write(
        project.join("contracts.toml"),
        render_content_contracts(&default_content_contracts(manifest.template)),
    )?;
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

fn default_memory_model(template: TemplateKind) -> MemoryModel<'static> {
    match template {
        TemplateKind::SingleScreenAction => MemoryModel {
            template,
            engine_region: BankRegion {
                name: "engine",
                start: "$80:8000",
                end: "$83:FFFF",
                purpose: "runtime code, common systems, scene loop, DMA scheduler",
            },
            content_region: BankRegion {
                name: "content",
                start: "$84:8000",
                end: "$9F:FFFF",
                purpose: "compiled graphics, maps, scripts, tables, audio descriptors",
            },
            work_ram: BankRegion {
                name: "wram",
                start: "$7E:0000",
                end: "$7F:FFFF",
                purpose: "entity state, transfer staging, scene state, decompression buffers",
            },
            vram: BankRegion {
                name: "vram",
                start: "$0000",
                end: "$7FFF",
                purpose: "background tiles, sprite tiles, dynamic upload windows",
            },
            cgram: BankRegion {
                name: "cgram",
                start: "$0000",
                end: "$01FF",
                purpose: "background and sprite palettes",
            },
            oam: BankRegion {
                name: "oam",
                start: "$0000",
                end: "$023F",
                purpose: "metasprite attribute staging and hardware sprite list",
            },
            dma_budget: DmaBudget {
                vram_bytes_per_frame: 4096,
                cgram_bytes_per_frame: 256,
                oam_bytes_per_frame: 544,
                max_transfers_per_frame: 8,
            },
        },
        other => MemoryModel {
            template: other,
            engine_region: BankRegion {
                name: "engine",
                start: "$80:8000",
                end: "$83:FFFF",
                purpose: "runtime code and fixed engine systems",
            },
            content_region: BankRegion {
                name: "content",
                start: "$84:8000",
                end: "$9F:FFFF",
                purpose: "compiled template content banks",
            },
            work_ram: BankRegion {
                name: "wram",
                start: "$7E:0000",
                end: "$7F:FFFF",
                purpose: "scene state, streaming buffers, entity state",
            },
            vram: BankRegion {
                name: "vram",
                start: "$0000",
                end: "$7FFF",
                purpose: "template-managed graphics windows",
            },
            cgram: BankRegion {
                name: "cgram",
                start: "$0000",
                end: "$01FF",
                purpose: "palette memory",
            },
            oam: BankRegion {
                name: "oam",
                start: "$0000",
                end: "$023F",
                purpose: "sprite attributes",
            },
            dma_budget: DmaBudget {
                vram_bytes_per_frame: 3072,
                cgram_bytes_per_frame: 256,
                oam_bytes_per_frame: 544,
                max_transfers_per_frame: 8,
            },
        },
    }
}

fn default_content_contracts(template: TemplateKind) -> ContentContracts<'static> {
    match template {
        TemplateKind::SingleScreenAction => ContentContracts {
            template,
            scenes: SceneContract {
                scene_format: "room text stub -> compiled room table",
                max_rooms: 64,
                tilemap_size: "32x32",
                background_layers: 1,
            },
            sprites: SpriteContract {
                sprite_format: "indexed PNG -> 4bpp tiles + metasprite frames",
                palette_format: "indexed PNG palette -> CGRAM set",
                max_sprite_tiles_per_room: 512,
                max_metasprites_per_room: 64,
            },
            entities: EntityContract {
                entity_format: "text stub -> entity table",
                max_entities_per_room: 24,
                player_slots: 1,
                script_hook_format: "boot/update/contact hooks",
            },
            audio: AudioContract {
                music_format: "named track references",
                sfx_format: "named sound-effect references",
                max_music_tracks: 16,
                max_sfx_ids: 64,
            },
        },
        other => ContentContracts {
            template: other,
            scenes: SceneContract {
                scene_format: "template-defined scene manifest",
                max_rooms: 128,
                tilemap_size: "template-defined",
                background_layers: 2,
            },
            sprites: SpriteContract {
                sprite_format: "indexed PNG -> compiled template sprite pack",
                palette_format: "palette file -> compiled CGRAM pack",
                max_sprite_tiles_per_room: 768,
                max_metasprites_per_room: 96,
            },
            entities: EntityContract {
                entity_format: "entity defs -> template entity table",
                max_entities_per_room: 32,
                player_slots: 1,
                script_hook_format: "template-defined script hooks",
            },
            audio: AudioContract {
                music_format: "track reference manifest",
                sfx_format: "effect reference manifest",
                max_music_tracks: 24,
                max_sfx_ids: 96,
            },
        },
    }
}

fn render_memory_model(model: &MemoryModel<'_>) -> String {
    format!(
        concat!(
            "template = \"{}\"\n\n",
            "[engine]\nname = \"{}\"\nstart = \"{}\"\nend = \"{}\"\npurpose = \"{}\"\n\n",
            "[content]\nname = \"{}\"\nstart = \"{}\"\nend = \"{}\"\npurpose = \"{}\"\n\n",
            "[wram]\nname = \"{}\"\nstart = \"{}\"\nend = \"{}\"\npurpose = \"{}\"\n\n",
            "[vram]\nname = \"{}\"\nstart = \"{}\"\nend = \"{}\"\npurpose = \"{}\"\n\n",
            "[cgram]\nname = \"{}\"\nstart = \"{}\"\nend = \"{}\"\npurpose = \"{}\"\n\n",
            "[oam]\nname = \"{}\"\nstart = \"{}\"\nend = \"{}\"\npurpose = \"{}\"\n\n",
            "[dma_budget]\nvram_bytes_per_frame = {}\ncgram_bytes_per_frame = {}\noam_bytes_per_frame = {}\nmax_transfers_per_frame = {}\n"
        ),
        template_kind_name(model.template),
        model.engine_region.name,
        model.engine_region.start,
        model.engine_region.end,
        model.engine_region.purpose,
        model.content_region.name,
        model.content_region.start,
        model.content_region.end,
        model.content_region.purpose,
        model.work_ram.name,
        model.work_ram.start,
        model.work_ram.end,
        model.work_ram.purpose,
        model.vram.name,
        model.vram.start,
        model.vram.end,
        model.vram.purpose,
        model.cgram.name,
        model.cgram.start,
        model.cgram.end,
        model.cgram.purpose,
        model.oam.name,
        model.oam.start,
        model.oam.end,
        model.oam.purpose,
        model.dma_budget.vram_bytes_per_frame,
        model.dma_budget.cgram_bytes_per_frame,
        model.dma_budget.oam_bytes_per_frame,
        model.dma_budget.max_transfers_per_frame
    )
}

fn render_content_contracts(contracts: &ContentContracts<'_>) -> String {
    format!(
        concat!(
            "template = \"{}\"\n\n",
            "[scenes]\nscene_format = \"{}\"\nmax_rooms = {}\ntilemap_size = \"{}\"\nbackground_layers = {}\n\n",
            "[sprites]\nsprite_format = \"{}\"\npalette_format = \"{}\"\nmax_sprite_tiles_per_room = {}\nmax_metasprites_per_room = {}\n\n",
            "[entities]\nentity_format = \"{}\"\nmax_entities_per_room = {}\nplayer_slots = {}\nscript_hook_format = \"{}\"\n\n",
            "[audio]\nmusic_format = \"{}\"\nsfx_format = \"{}\"\nmax_music_tracks = {}\nmax_sfx_ids = {}\n"
        ),
        template_kind_name(contracts.template),
        contracts.scenes.scene_format,
        contracts.scenes.max_rooms,
        contracts.scenes.tilemap_size,
        contracts.scenes.background_layers,
        contracts.sprites.sprite_format,
        contracts.sprites.palette_format,
        contracts.sprites.max_sprite_tiles_per_room,
        contracts.sprites.max_metasprites_per_room,
        contracts.entities.entity_format,
        contracts.entities.max_entities_per_room,
        contracts.entities.player_slots,
        contracts.entities.script_hook_format,
        contracts.audio.music_format,
        contracts.audio.sfx_format,
        contracts.audio.max_music_tracks,
        contracts.audio.max_sfx_ids
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
    for file in REQUIRED_FILES {
        if !project.join(file).is_file() {
            issues.push(format!("missing required file `{file}`"));
        }
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
    out.push('\n');
    for file in ["memory.toml", "contracts.toml"] {
        let bytes = fs::read_to_string(project.join(file))?.len();
        out.push_str(&format!("{file}: {bytes} bytes\n"));
    }
    Ok(out)
}

fn render_project_readme(manifest: &GameManifest) -> String {
    format!(
        "# {}\n\nTemplate: `{}`\n\nThis project was initialized by `template init`.\n\nKey files:\n\n- `game.toml`: project manifest\n- `memory.toml`: cartridge memory layout and DMA budgets\n- `contracts.toml`: content limits and compile-time contracts\n\nNext steps:\n\n1. review `memory.toml` and `contracts.toml`\n2. add placeholder assets under `assets/`\n3. define your first room in `scenes/room_000.txt`\n4. run `cargo run -- template validate --project {}`\n5. run `cargo run -- template build --project {} --out build/{}`\n",
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
    use super::{
        GameManifest, TemplateKind, default_content_contracts, default_memory_model, parse_manifest,
        render_content_contracts, render_manifest, render_memory_model, title_case,
        validate_project_layout,
    };
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

    #[test]
    fn memory_model_render_mentions_engine_and_dma_budget() {
        let text = render_memory_model(&default_memory_model(TemplateKind::SingleScreenAction));
        assert!(text.contains("[engine]"));
        assert!(text.contains("vram_bytes_per_frame = 4096"));
    }

    #[test]
    fn content_contract_render_mentions_scene_limits() {
        let text =
            render_content_contracts(&default_content_contracts(TemplateKind::SingleScreenAction));
        assert!(text.contains("[scenes]"));
        assert!(text.contains("max_entities_per_room = 24"));
    }
}
