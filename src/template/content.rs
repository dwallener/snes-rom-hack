use super::{ContentContracts, GameManifest, TemplateKind};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SceneDef {
    pub id: String,
    pub kind: String,
    pub background: String,
    pub palette: String,
    pub music: String,
    pub player_spawn: String,
    pub enemy_set: String,
    pub next_scene: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EntityDef {
    pub id: String,
    pub kind: String,
    pub sprite_page: String,
    pub palette: String,
    pub hitbox: String,
    pub speed: u16,
    pub jump: u16,
    pub attack: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ScriptDef {
    pub on_boot: String,
    pub on_game_over: String,
    pub on_room_clear: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CompiledContent {
    pub template: TemplateKind,
    pub game: String,
    pub title_scene: String,
    pub scenes: Vec<SceneDef>,
    pub entities: Vec<EntityDef>,
    pub script: ScriptDef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RoomAssetRecord {
    pub scene_id: String,
    pub background: String,
    pub palette: String,
    pub music: String,
    pub enemy_set: String,
    pub next_scene: String,
}

pub(crate) fn load_compiled_content(
    project: &Path,
    manifest: &GameManifest,
    contracts: &ContentContracts<'_>,
) -> io::Result<CompiledContent> {
    let scenes = load_scenes(&project.join("scenes"))?;
    let entities = load_entities(&project.join("entities"))?;
    let script = load_script(&project.join("scripts/main.toml"))?;

    validate_content(manifest, contracts, &scenes, &entities, &script)?;

    let title_scene = scenes
        .iter()
        .find(|scene| scene.kind == "title")
        .map(|scene| scene.id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing title scene"))?;

    Ok(CompiledContent {
        template: manifest.template,
        game: manifest.name.clone(),
        title_scene,
        scenes,
        entities,
        script,
    })
}

pub(crate) fn build_room_asset_table(content: &CompiledContent) -> Vec<RoomAssetRecord> {
    content
        .scenes
        .iter()
        .map(|scene| RoomAssetRecord {
            scene_id: scene.id.clone(),
            background: scene.background.clone(),
            palette: scene.palette.clone(),
            music: scene.music.clone(),
            enemy_set: scene.enemy_set.clone(),
            next_scene: scene.next_scene.clone(),
        })
        .collect()
}

pub(crate) fn render_content_summary(content: &CompiledContent) -> String {
    let mut out = String::new();
    out.push_str("Compiled Template Content\n");
    out.push_str(&format!("game: {}\n", content.game));
    out.push_str(&format!("template: {:?}\n", content.template));
    out.push_str(&format!("title_scene: {}\n\n", content.title_scene));

    out.push_str("Scenes\n");
    for scene in &content.scenes {
        out.push_str(&format!(
            "- {} [{}] bg={} palette={} music={} next={}\n",
            scene.id, scene.kind, scene.background, scene.palette, scene.music, scene.next_scene
        ));
    }

    out.push_str("\nEntities\n");
    for entity in &content.entities {
        out.push_str(&format!(
            "- {} kind={} sprite_page={} palette={} attack={}\n",
            entity.id, entity.kind, entity.sprite_page, entity.palette, entity.attack
        ));
    }

    out.push_str("\nScript Hooks\n");
    out.push_str(&format!("- on_boot: {}\n", content.script.on_boot));
    out.push_str(&format!("- on_game_over: {}\n", content.script.on_game_over));
    out.push_str(&format!("- on_room_clear: {}\n", content.script.on_room_clear));
    out
}

fn validate_content(
    manifest: &GameManifest,
    contracts: &ContentContracts<'_>,
    scenes: &[SceneDef],
    entities: &[EntityDef],
    script: &ScriptDef,
) -> io::Result<()> {
    if scenes.len() > usize::from(contracts.scenes.max_rooms) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "scene count {} exceeds max_rooms {}",
                scenes.len(),
                contracts.scenes.max_rooms
            ),
        ));
    }
    if entities.len() > usize::from(contracts.entities.max_entities_per_room) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "entity count {} exceeds max_entities_per_room {}",
                entities.len(),
                contracts.entities.max_entities_per_room
            ),
        ));
    }
    if !scenes.iter().any(|scene| scene.kind == "title") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected at least one title scene",
        ));
    }
    if !scenes.iter().any(|scene| scene.kind == "gameplay") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected at least one gameplay scene",
        ));
    }
    if !entities.iter().any(|entity| entity.kind == "player") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected one player entity definition",
        ));
    }
    if !script.on_boot.starts_with("load_scene ") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "on_boot must start with `load_scene `",
        ));
    }

    if manifest.template == TemplateKind::SingleScreenAction {
        for scene in scenes {
            if scene.palette.is_empty() || scene.background.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("scene `{}` is missing background or palette", scene.id),
                ));
            }
        }
    }

    Ok(())
}

fn load_scenes(dir: &Path) -> io::Result<Vec<SceneDef>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let map = parse_flat_kv_file(&path)?;
        out.push(SceneDef {
            id: required_string(&map, "id", &path)?,
            kind: required_string(&map, "kind", &path)?,
            background: required_string(&map, "background", &path)?,
            palette: required_string(&map, "palette", &path)?,
            music: required_string(&map, "music", &path)?,
            player_spawn: required_string(&map, "player_spawn", &path)?,
            enemy_set: required_string(&map, "enemy_set", &path)?,
            next_scene: required_string(&map, "next_scene", &path)?,
            source_file: path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn load_entities(dir: &Path) -> io::Result<Vec<EntityDef>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let map = parse_flat_kv_file(&path)?;
        out.push(EntityDef {
            id: required_string(&map, "id", &path)?,
            kind: required_string(&map, "kind", &path)?,
            sprite_page: required_string(&map, "sprite_page", &path)?,
            palette: required_string(&map, "palette", &path)?,
            hitbox: required_string(&map, "hitbox", &path)?,
            speed: required_u16(&map, "speed", &path)?,
            jump: required_u16(&map, "jump", &path)?,
            attack: required_string(&map, "attack", &path)?,
            source_file: path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn load_script(path: &Path) -> io::Result<ScriptDef> {
    let map = parse_flat_kv_file(path)?;
    Ok(ScriptDef {
        on_boot: required_string(&map, "on_boot", path)?,
        on_game_over: required_string(&map, "on_game_over", path)?,
        on_room_clear: required_string(&map, "on_room_clear", path)?,
        source_file: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
    })
}

fn parse_flat_kv_file(path: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let text = fs::read_to_string(path)?;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        out.insert(
            key.trim().to_string(),
            value.trim().trim_matches('"').to_string(),
        );
    }
    Ok(out)
}

fn required_string(
    map: &BTreeMap<String, String>,
    key: &str,
    path: &Path,
) -> io::Result<String> {
    map.get(key).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing key `{key}` in {}", path.display()),
        )
    })
}

fn required_u16(map: &BTreeMap<String, String>, key: &str, path: &Path) -> io::Result<u16> {
    let raw = required_string(map, key, path)?;
    raw.parse::<u16>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid integer for `{key}` in {}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{build_room_asset_table, load_compiled_content, render_content_summary};
    use crate::template::{
        GameManifest, TemplateKind, default_content_contracts, render_entity_stub, render_scene_stub,
        render_script_stub,
    };
    use std::fs;

    #[test]
    fn loads_stub_project_into_compiled_content() {
        let temp = std::env::temp_dir().join(format!("template-content-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("scenes")).expect("scenes");
        fs::create_dir_all(temp.join("entities")).expect("entities");
        fs::create_dir_all(temp.join("scripts")).expect("scripts");
        fs::write(
            temp.join("scenes/title_room.toml"),
            render_scene_stub("title_room", "bg_title", "12,14", "title_theme", true),
        )
        .expect("title scene");
        fs::write(
            temp.join("scenes/room_000.toml"),
            render_scene_stub("room_000", "bg_main", "8,8", "stage_01", false),
        )
        .expect("room scene");
        fs::write(
            temp.join("entities/player.toml"),
            render_entity_stub("player", "player", "ball_player", "player_ball", 2, 4, "basic"),
        )
        .expect("entity");
        fs::write(temp.join("scripts/main.toml"), render_script_stub()).expect("script");

        let manifest = GameManifest {
            name: "demo".to_string(),
            template: TemplateKind::SingleScreenAction,
            title: "Demo".to_string(),
            region: "ntsc".to_string(),
            version: "0.1.0".to_string(),
        };
        let contracts = default_content_contracts(TemplateKind::SingleScreenAction);
        let compiled = load_compiled_content(&temp, &manifest, &contracts).expect("compiled");
        assert_eq!(compiled.title_scene, "title_room");
        assert_eq!(compiled.scenes.len(), 2);
        assert_eq!(compiled.entities.len(), 1);

        let summary = render_content_summary(&compiled);
        assert!(summary.contains("title_scene: title_room"));
        let table = build_room_asset_table(&compiled);
        assert_eq!(table.len(), 2);

        let _ = fs::remove_dir_all(&temp);
    }
}
