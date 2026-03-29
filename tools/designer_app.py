from __future__ import annotations

import subprocess
from pathlib import Path
import tomllib

import streamlit as st


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PROJECT = ROOT / "games" / "arena-demo"
DEFAULT_BUILD = ROOT / "out" / "designer-build"
DEFAULT_SIM = ROOT / "out" / "designer-sim"


def run_cmd(args: list[str]) -> tuple[int, str]:
    proc = subprocess.run(
        args,
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    out = proc.stdout
    if proc.stderr:
        out = f"{out}\n{proc.stderr}".strip()
    return proc.returncode, out.strip()


def load_toml(path: Path) -> dict:
    if not path.is_file():
        return {}
    with path.open("rb") as handle:
        return tomllib.load(handle)


def asset_names(project: Path, category: str) -> list[str]:
    base = project / "assets" / category
    names = []
    for path in sorted(base.glob("*.toml")):
        data = load_toml(path)
        if "name" in data:
            names.append(str(data["name"]))
    return names


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def save_manifest(project: Path, name: str, title: str) -> None:
    write_text(
        project / "game.toml",
        (
            f'name = "{name}"\n'
            'template = "single-screen-action"\n'
            f'title = "{title}"\n'
            'region = "ntsc"\n'
            'version = "0.1.0"\n'
        ),
    )


def save_scene(project: Path, scene_id: str, kind: str, background: str, music: str, spawn_x: int, spawn_y: int, next_scene: str) -> None:
    write_text(
        project / "scenes" / f"{scene_id}.toml",
        (
            f'id = "{scene_id}"\n'
            f'kind = "{kind}"\n'
            f'background = "{background}"\n'
            'palette = "default"\n'
            f'music = "{music}"\n'
            f'player_spawn = "{spawn_x},{spawn_y}"\n'
            f'enemy_set = "{"none" if kind == "title" else "arena_enemies"}"\n'
            f'next_scene = "{next_scene}"\n'
        ),
    )


def save_entity(project: Path, filename: str, entity_id: str, kind: str, sprite_page: str, palette: str, speed: int, attack: str) -> None:
    write_text(
        project / "entities" / filename,
        (
            f'id = "{entity_id}"\n'
            f'kind = "{kind}"\n'
            f'sprite_page = "{sprite_page}"\n'
            f'palette = "{palette}"\n'
            'hitbox = "8,8,16,16"\n'
            f"speed = {speed}\n"
            'jump = 0\n'
            f'attack = "{attack}"\n'
        ),
    )


def save_script(project: Path, title_scene: str, arena_scene: str) -> None:
    write_text(
        project / "scripts" / "main.toml",
        (
            f'on_boot = "load_scene {title_scene}"\n'
            f'on_game_over = "load_scene {title_scene}"\n'
            f'on_room_clear = "load_scene {arena_scene}"\n'
        ),
    )


def project_exists(project: Path) -> bool:
    return (project / "game.toml").is_file()


def load_entity(project: Path, filename: str) -> dict:
    return load_toml(project / "entities" / filename)


def load_scene(project: Path, filename: str) -> dict:
    return load_toml(project / "scenes" / filename)


def display_preview_images(path: Path, header: str) -> None:
    if not path.exists():
        return
    files = sorted(path.glob("*.png"))
    if not files:
        return
    st.subheader(header)
    cols = st.columns(min(3, len(files)))
    for idx, image in enumerate(files):
        cols[idx % len(cols)].image(str(image), caption=image.name)


st.set_page_config(page_title="SNES Arena Designer", layout="wide")
st.title("SNES Arena Designer")
st.caption("Single-screen action v1: one arena, one player, one enemy archetype, defeat all enemies.")

with st.sidebar:
    st.header("Project")
    project_path = Path(st.text_input("Project Folder", str(DEFAULT_PROJECT)))
    build_out = Path(st.text_input("Build Output", str(DEFAULT_BUILD)))
    sim_out = Path(st.text_input("Sim Output", str(DEFAULT_SIM)))
    if st.button("Initialize/Open Project", use_container_width=True):
        if not project_exists(project_path):
            code, output = run_cmd(
                [
                    "cargo",
                    "run",
                    "-q",
                    "--",
                    "template",
                    "init",
                    "--kind",
                    "single-screen-action",
                    "--out",
                    str(project_path),
                ]
            )
            st.session_state["init_log"] = output
            if code != 0:
                st.error(output or "init failed")
            else:
                st.success(f"project ready at {project_path}")
        else:
            st.session_state["init_log"] = f"opened existing project {project_path}"
            st.success(f"opened existing project {project_path}")
    if "init_log" in st.session_state:
        st.code(st.session_state["init_log"], language="text")

if not project_exists(project_path):
    st.info("Initialize a project first.")
    st.stop()

manifest = load_toml(project_path / "game.toml")
title_scene = load_scene(project_path, "title_room.toml")
arena_scene = load_scene(project_path, "room_000.toml")
player = load_entity(project_path, "player.toml")
npc = load_entity(project_path, "npc_ball.toml")

backgrounds = asset_names(project_path, "backgrounds")
music = asset_names(project_path, "audio")
sprites = asset_names(project_path, "sprites")
palettes = asset_names(project_path, "palettes")

tab_project, tab_assets, tab_arena, tab_actors, tab_run = st.tabs(
    ["Project", "Assets", "Arena", "Actors", "Build & Simulate"]
)

with tab_project:
    name = st.text_input("Project Name", str(manifest.get("name", "arena-demo")))
    title = st.text_input("Game Title", str(manifest.get("title", "Arena Demo")))
    if st.button("Save Project"):
        save_manifest(project_path, name, title)
        st.success("saved game.toml")

with tab_assets:
    st.write("Current asset definitions come from `assets/**/*.toml`.")
    st.write(f"Backgrounds: {', '.join(backgrounds) or 'none'}")
    st.write(f"Music: {', '.join(music) or 'none'}")
    st.write(f"Sprites: {', '.join(sprites) or 'none'}")
    st.write(f"Palettes: {', '.join(palettes) or 'none'}")

with tab_arena:
    title_bg = st.selectbox("Title Background", backgrounds, index=max(0, backgrounds.index(title_scene.get("background", backgrounds[0])) if backgrounds else 0))
    title_music = st.selectbox("Title Music", music, index=max(0, music.index(title_scene.get("music", music[0])) if music else 0))
    arena_bg = st.selectbox("Arena Background", backgrounds, index=max(0, backgrounds.index(arena_scene.get("background", backgrounds[0])) if backgrounds else 0), key="arena_bg")
    arena_music = st.selectbox("Arena Music", music, index=max(0, music.index(arena_scene.get("music", music[0])) if music else 0), key="arena_music")
    spawn_raw = str(arena_scene.get("player_spawn", "8,8")).split(",")
    spawn_x = st.number_input("Player Spawn X", min_value=0, max_value=31, value=int(spawn_raw[0]))
    spawn_y = st.number_input("Player Spawn Y", min_value=0, max_value=27, value=int(spawn_raw[1]))
    if st.button("Save Arena"):
        save_scene(project_path, "title_room", "title", title_bg, title_music, 12, 14, "room_000")
        save_scene(project_path, "room_000", "gameplay", arena_bg, arena_music, int(spawn_x), int(spawn_y), "room_000")
        save_script(project_path, "title_room", "room_000")
        st.success("saved title and arena scenes")

with tab_actors:
    player_sprite = st.selectbox("Player Sprite", sprites, index=max(0, sprites.index(player.get("sprite_page", sprites[0])) if sprites else 0))
    player_palette = st.selectbox("Player Palette", palettes, index=max(0, palettes.index(player.get("palette", palettes[0])) if palettes else 0))
    player_speed = st.slider("Player Speed", min_value=1, max_value=4, value=int(player.get("speed", 2)))
    npc_sprite = st.selectbox("NPC Sprite", sprites, index=max(0, sprites.index(npc.get("sprite_page", sprites[0])) if sprites else 0), key="npc_sprite")
    npc_palette = st.selectbox("NPC Palette", palettes, index=max(0, palettes.index(npc.get("palette", palettes[0])) if palettes else 0), key="npc_palette")
    npc_speed = st.slider("NPC Speed", min_value=1, max_value=3, value=int(npc.get("speed", 1)))
    if st.button("Save Actors"):
        save_entity(project_path, "player.toml", "player", "player", player_sprite, player_palette, int(player_speed), "basic")
        save_entity(project_path, "npc_ball.toml", "npc_ball", "npc", npc_sprite, npc_palette, int(npc_speed), "touch")
        st.success("saved actor definitions")

with tab_run:
    sim_input = st.text_input("Simulation Input", "RRRRDDLLUU..RR")
    col1, col2, col3 = st.columns(3)
    if col1.button("Validate", use_container_width=True):
        code, output = run_cmd(["cargo", "run", "-q", "--", "template", "validate", "--project", str(project_path)])
        st.code(output, language="text")
        if code == 0:
            st.success("validate ok")
        else:
            st.error("validate failed")
    if col2.button("Build", use_container_width=True):
        code, output = run_cmd(
            ["cargo", "run", "-q", "--", "template", "build", "--project", str(project_path), "--out", str(build_out)]
        )
        st.code(output, language="text")
        if code == 0:
            st.success("build ok")
        else:
            st.error("build failed")
    rom_files = sorted(build_out.glob("*.sfc"))
    if rom_files:
        st.subheader("ROM Output")
        for rom in rom_files:
            st.code(str(rom), language="text")
    if col3.button("Simulate", use_container_width=True):
        code, output = run_cmd(
            [
                "cargo",
                "run",
                "-q",
                "--",
                "template",
                "simulate",
                "--project",
                str(project_path),
                "--out",
                str(sim_out),
                "--input",
                sim_input,
            ]
        )
        st.code(output, language="text")
        if code == 0:
            st.success("simulate ok")
        else:
            st.error("simulate failed")

    if (build_out / "engine" / "engine_summary.txt").is_file():
        st.subheader("Engine Summary")
        st.code((build_out / "engine" / "engine_summary.txt").read_text(encoding="utf-8"), language="text")
    if (sim_out / "simulation_summary.txt").is_file():
        st.subheader("Simulation Summary")
        st.code((sim_out / "simulation_summary.txt").read_text(encoding="utf-8"), language="text")

    display_preview_images(build_out / "content" / "previews", "Scene Previews")
    display_preview_images(build_out / "assets" / "compiled", "Sprite Previews")
    display_preview_images(sim_out / "frames", "Simulation Frames")
