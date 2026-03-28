use super::TemplateKind;
use crate::template::content::{CompiledContent, RoomAssetRecord};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AssetBundle {
    pub template: TemplateKind,
    pub backgrounds: Vec<BackgroundAsset>,
    pub palettes: Vec<PaletteAsset>,
    pub sprite_pages: Vec<SpritePageAsset>,
    pub audio_tracks: Vec<AudioAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BackgroundAsset {
    pub id: u16,
    pub name: String,
    pub source: String,
    pub palette: String,
    pub vram_slot: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PaletteAsset {
    pub id: u16,
    pub name: String,
    pub source: String,
    pub cgram_slot: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SpritePageAsset {
    pub id: u16,
    pub name: String,
    pub source: String,
    pub palette: String,
    pub vram_slot: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AudioAsset {
    pub id: u16,
    pub name: String,
    pub source: String,
    pub kind: String,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ResolvedRoomAssetRecord {
    pub scene_id: String,
    pub background_id: u16,
    pub palette_id: u16,
    pub music_id: u16,
    pub next_scene: String,
    pub background_vram_slot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ResolvedEntityAssetRecord {
    pub entity_id: String,
    pub sprite_page_id: u16,
    pub sprite_vram_slot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AssetResolution {
    pub rooms: Vec<ResolvedRoomAssetRecord>,
    pub entities: Vec<ResolvedEntityAssetRecord>,
}

pub(crate) fn load_asset_bundle(project: &Path, template: TemplateKind) -> io::Result<AssetBundle> {
    let bundle = AssetBundle {
        template,
        backgrounds: load_backgrounds(&project.join("assets/backgrounds"))?,
        palettes: load_palettes(&project.join("assets/palettes"))?,
        sprite_pages: load_sprite_pages(&project.join("assets/sprites"))?,
        audio_tracks: load_audio(&project.join("assets/audio"))?,
    };
    validate_asset_bundle(&bundle)?;
    Ok(bundle)
}

pub(crate) fn resolve_asset_references(
    content: &CompiledContent,
    assets: &AssetBundle,
    room_table: &[RoomAssetRecord],
) -> io::Result<AssetResolution> {
    let background_ids = map_by_name_background(&assets.backgrounds);
    let palette_ids = map_by_name_palette(&assets.palettes);
    let audio_ids = map_by_name_audio(&assets.audio_tracks);
    let sprite_ids = map_by_name_sprite(&assets.sprite_pages);

    let mut rooms = Vec::new();
    for room in room_table {
        let background = background_ids.get(room.background.as_str()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown background `{}` for scene `{}`", room.background, room.scene_id),
            )
        })?;
        let palette = palette_ids.get(room.palette.as_str()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown palette `{}` for scene `{}`", room.palette, room.scene_id),
            )
        })?;
        let music = audio_ids.get(room.music.as_str()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown music `{}` for scene `{}`", room.music, room.scene_id),
            )
        })?;
        rooms.push(ResolvedRoomAssetRecord {
            scene_id: room.scene_id.clone(),
            background_id: background.id,
            palette_id: palette.id,
            music_id: music.id,
            next_scene: room.next_scene.clone(),
            background_vram_slot: background.vram_slot.clone(),
        });
    }

    let mut entities = Vec::new();
    for entity in &content.entities {
        let sprite = sprite_ids.get(entity.sprite_page.as_str()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unknown sprite_page `{}` for entity `{}`",
                    entity.sprite_page, entity.id
                ),
            )
        })?;
        entities.push(ResolvedEntityAssetRecord {
            entity_id: entity.id.clone(),
            sprite_page_id: sprite.id,
            sprite_vram_slot: sprite.vram_slot.clone(),
        });
    }

    Ok(AssetResolution { rooms, entities })
}

pub(crate) fn render_asset_summary(assets: &AssetBundle, resolution: &AssetResolution) -> String {
    let mut out = String::new();
    out.push_str("Compiled Asset Tables\n");
    out.push_str(&format!("template: {:?}\n\n", assets.template));

    out.push_str("Backgrounds\n");
    for asset in &assets.backgrounds {
        out.push_str(&format!(
            "- #{} {} source={} palette={} slot={}\n",
            asset.id, asset.name, asset.source, asset.palette, asset.vram_slot
        ));
    }

    out.push_str("\nPalettes\n");
    for asset in &assets.palettes {
        out.push_str(&format!(
            "- #{} {} source={} slot={}\n",
            asset.id, asset.name, asset.source, asset.cgram_slot
        ));
    }

    out.push_str("\nSprite Pages\n");
    for asset in &assets.sprite_pages {
        out.push_str(&format!(
            "- #{} {} source={} palette={} slot={}\n",
            asset.id, asset.name, asset.source, asset.palette, asset.vram_slot
        ));
    }

    out.push_str("\nAudio\n");
    for asset in &assets.audio_tracks {
        out.push_str(&format!(
            "- #{} {} kind={} source={}\n",
            asset.id, asset.name, asset.kind, asset.source
        ));
    }

    out.push_str("\nResolved Room Assets\n");
    for room in &resolution.rooms {
        out.push_str(&format!(
            "- {} bg={} palette={} music={} slot={}\n",
            room.scene_id, room.background_id, room.palette_id, room.music_id, room.background_vram_slot
        ));
    }

    out.push_str("\nResolved Entity Assets\n");
    for entity in &resolution.entities {
        out.push_str(&format!(
            "- {} sprite_page={} slot={}\n",
            entity.entity_id, entity.sprite_page_id, entity.sprite_vram_slot
        ));
    }
    out
}

fn load_backgrounds(dir: &Path) -> io::Result<Vec<BackgroundAsset>> {
    let mut out = Vec::new();
    for (id, path) in sorted_toml_paths(dir)?.into_iter().enumerate() {
        let map = parse_flat_kv_file(&path)?;
        out.push(BackgroundAsset {
            id: id as u16,
            name: required_string(&map, "name", &path)?,
            source: required_string(&map, "source", &path)?,
            palette: required_string(&map, "palette", &path)?,
            vram_slot: required_string(&map, "vram_slot", &path)?,
            source_file: file_name_string(&path),
        });
    }
    Ok(out)
}

fn load_palettes(dir: &Path) -> io::Result<Vec<PaletteAsset>> {
    let mut out = Vec::new();
    for (id, path) in sorted_toml_paths(dir)?.into_iter().enumerate() {
        let map = parse_flat_kv_file(&path)?;
        out.push(PaletteAsset {
            id: id as u16,
            name: required_string(&map, "name", &path)?,
            source: required_string(&map, "source", &path)?,
            cgram_slot: required_string(&map, "cgram_slot", &path)?,
            source_file: file_name_string(&path),
        });
    }
    Ok(out)
}

fn load_sprite_pages(dir: &Path) -> io::Result<Vec<SpritePageAsset>> {
    let mut out = Vec::new();
    for (id, path) in sorted_toml_paths(dir)?.into_iter().enumerate() {
        let map = parse_flat_kv_file(&path)?;
        out.push(SpritePageAsset {
            id: id as u16,
            name: required_string(&map, "name", &path)?,
            source: required_string(&map, "source", &path)?,
            palette: required_string(&map, "palette", &path)?,
            vram_slot: required_string(&map, "vram_slot", &path)?,
            source_file: file_name_string(&path),
        });
    }
    Ok(out)
}

fn load_audio(dir: &Path) -> io::Result<Vec<AudioAsset>> {
    let mut out = Vec::new();
    for (id, path) in sorted_toml_paths(dir)?.into_iter().enumerate() {
        let map = parse_flat_kv_file(&path)?;
        out.push(AudioAsset {
            id: id as u16,
            name: required_string(&map, "name", &path)?,
            source: required_string(&map, "source", &path)?,
            kind: required_string(&map, "kind", &path)?,
            source_file: file_name_string(&path),
        });
    }
    Ok(out)
}

fn validate_asset_bundle(bundle: &AssetBundle) -> io::Result<()> {
    let palette_names: BTreeMap<&str, &PaletteAsset> =
        bundle.palettes.iter().map(|item| (item.name.as_str(), item)).collect();
    for background in &bundle.backgrounds {
        if !palette_names.contains_key(background.palette.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "background `{}` references unknown palette `{}`",
                    background.name, background.palette
                ),
            ));
        }
    }
    for sprite in &bundle.sprite_pages {
        if !palette_names.contains_key(sprite.palette.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "sprite page `{}` references unknown palette `{}`",
                    sprite.name, sprite.palette
                ),
            ));
        }
    }
    for audio in &bundle.audio_tracks {
        if audio.kind != "music" && audio.kind != "sfx" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("audio `{}` has unsupported kind `{}`", audio.name, audio.kind),
            ));
        }
    }
    Ok(())
}

fn sorted_toml_paths(dir: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) == Some("toml") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
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

fn file_name_string(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn map_by_name_background(items: &[BackgroundAsset]) -> BTreeMap<&str, &BackgroundAsset> {
    items.iter().map(|item| (item.name.as_str(), item)).collect()
}

fn map_by_name_palette(items: &[PaletteAsset]) -> BTreeMap<&str, &PaletteAsset> {
    items.iter().map(|item| (item.name.as_str(), item)).collect()
}

fn map_by_name_sprite(items: &[SpritePageAsset]) -> BTreeMap<&str, &SpritePageAsset> {
    items.iter().map(|item| (item.name.as_str(), item)).collect()
}

fn map_by_name_audio(items: &[AudioAsset]) -> BTreeMap<&str, &AudioAsset> {
    items.iter().map(|item| (item.name.as_str(), item)).collect()
}

#[cfg(test)]
mod tests {
    use super::{load_asset_bundle, render_asset_summary, resolve_asset_references};
    use crate::template::content::{build_room_asset_table, load_compiled_content};
    use crate::template::{
        GameManifest, TemplateKind, default_content_contracts, render_audio_asset_stub,
        render_background_asset_stub, render_entity_stub, render_palette_asset_stub, render_scene_stub,
        render_script_stub, render_sprite_asset_stub,
    };
    use std::fs;

    #[test]
    fn resolves_stub_content_against_stub_assets() {
        let temp = std::env::temp_dir().join(format!("template-assets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("assets/backgrounds")).expect("backgrounds");
        fs::create_dir_all(temp.join("assets/palettes")).expect("palettes");
        fs::create_dir_all(temp.join("assets/sprites")).expect("sprites");
        fs::create_dir_all(temp.join("assets/audio")).expect("audio");
        fs::create_dir_all(temp.join("scenes")).expect("scenes");
        fs::create_dir_all(temp.join("entities")).expect("entities");
        fs::create_dir_all(temp.join("scripts")).expect("scripts");

        fs::write(
            temp.join("assets/backgrounds/bg_title.toml"),
            render_background_asset_stub("bg_title", "title.png", "default", "bg_tiles"),
        )
        .expect("bg title");
        fs::write(
            temp.join("assets/backgrounds/bg_main.toml"),
            render_background_asset_stub("bg_main", "main.png", "default", "bg_tiles"),
        )
        .expect("bg main");
        fs::write(
            temp.join("assets/palettes/default.toml"),
            render_palette_asset_stub("default", "default.pal", "palette0"),
        )
        .expect("palette");
        fs::write(
            temp.join("assets/sprites/hero_main.toml"),
            render_sprite_asset_stub("hero_main", "hero.png", "default", "sprite_tiles"),
        )
        .expect("sprite");
        fs::write(
            temp.join("assets/audio/title_theme.toml"),
            render_audio_asset_stub("title_theme", "title.spc", "music"),
        )
        .expect("audio title");
        fs::write(
            temp.join("assets/audio/stage_01.toml"),
            render_audio_asset_stub("stage_01", "stage1.spc", "music"),
        )
        .expect("audio stage");
        fs::write(
            temp.join("scenes/title_room.toml"),
            render_scene_stub("title_room", "bg_title", "12,14", "title_theme", true),
        )
        .expect("scene title");
        fs::write(
            temp.join("scenes/room_000.toml"),
            render_scene_stub("room_000", "bg_main", "8,8", "stage_01", false),
        )
        .expect("scene room");
        fs::write(temp.join("entities/player.toml"), render_entity_stub()).expect("entity");
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
        let bundle = load_asset_bundle(&temp, TemplateKind::SingleScreenAction).expect("bundle");
        let resolution =
            resolve_asset_references(&compiled, &bundle, &build_room_asset_table(&compiled))
                .expect("resolution");
        let summary = render_asset_summary(&bundle, &resolution);
        assert!(summary.contains("Resolved Room Assets"));
        assert_eq!(resolution.rooms.len(), 2);
        assert_eq!(resolution.entities.len(), 1);

        let _ = fs::remove_dir_all(&temp);
    }
}
