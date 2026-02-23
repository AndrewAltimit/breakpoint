#!/usr/bin/env python3
"""
Build the 512x512 platformer sprite atlas and parallax background texture.

Reads GothicVania source sprite sheets from assets/gothicvania/ if present,
otherwise generates programmatic placeholder sprites with distinct colors per
sprite type. Outputs:
  - web/assets/sprites/platformer_atlas.png  (512x512 RGBA)
  - web/assets/sprites/platformer_atlas.json (sprite name -> rect mapping)
  - web/assets/sprites/platformer_bg.png     (512x512, 3 layers stacked)

Usage:
    python scripts/build_atlas.py
"""

import json
import os
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:
    print("Pillow is required: pip install Pillow", file=sys.stderr)
    sys.exit(1)

ATLAS_SIZE = 512
CELL = 16  # Base cell size
OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "web", "assets", "sprites")

# Sprite definitions: (name, x, y, w, h)
# Organized by atlas region

SPRITES = []


def add_frames(prefix, count, x_start, y, w, h):
    """Add numbered animation frames."""
    for i in range(count):
        SPRITES.append((f"{prefix}_{i}", x_start + i * w, y, w, h))


# --- Row 0-3 (Y 0-63): Player sprites (16x32 each) ---
# Idle: 6 frames
add_frames("player_idle", 6, 0, 0, 16, 32)
# Walk: 6 frames
add_frames("player_walk", 6, 96, 0, 16, 32)
# Jump: 3 frames
add_frames("player_jump", 3, 192, 0, 16, 32)
# Fall: 3 frames
add_frames("player_fall", 3, 240, 0, 16, 32)
# Attack: 6 frames
add_frames("player_attack", 6, 288, 0, 16, 32)
# Hurt: 2 frames
add_frames("player_hurt", 2, 384, 0, 16, 32)
# Dead: 4 frames
add_frames("player_dead", 4, 416, 0, 16, 32)

# --- Row 4-7 (Y 64-127): Enemy sprites (16x32 each) ---
# Skeleton walk: 4 frames
add_frames("skeleton_walk", 4, 0, 64, 16, 32)
# Skeleton attack: 3 frames
add_frames("skeleton_attack", 3, 64, 64, 16, 32)
# Skeleton death: 4 frames
add_frames("skeleton_death", 4, 112, 64, 16, 32)
# Bat fly: 4 frames
add_frames("bat_fly", 4, 176, 64, 16, 32)
# Bat death: 2 frames
add_frames("bat_death", 2, 240, 64, 16, 32)
# Knight walk: 4 frames
add_frames("knight_walk", 4, 272, 64, 16, 32)
# Knight attack: 3 frames
add_frames("knight_attack", 3, 336, 64, 16, 32)
# Knight death: 4 frames
add_frames("knight_death", 4, 384, 64, 16, 32)
# Medusa float: 4 frames
add_frames("medusa_float", 4, 0, 96, 16, 32)
# Medusa death: 2 frames
add_frames("medusa_death", 2, 64, 96, 16, 32)
# Projectile: 3 frames
add_frames("projectile", 3, 96, 96, 16, 16)

# --- Row 8-11 (Y 128-191): Tiles (16x16 each) ---
tile_x = 0
tile_y = 128
# Stone brick variants
for name in [
    "stone_brick_top",
    "stone_brick_inner",
    "stone_brick_left",
    "stone_brick_right",
    "stone_brick_top_left",
    "stone_brick_top_right",
    "stone_brick_bottom_left",
    "stone_brick_bottom_right",
]:
    SPRITES.append((name, tile_x, tile_y, 16, 16))
    tile_x += 16

# Platform variants: 3
add_frames("platform", 3, tile_x, tile_y, 16, 16)
tile_x += 48

# Spikes: 2 variants
add_frames("spikes", 2, tile_x, tile_y, 16, 16)
tile_x += 32

# Second row of tiles (Y 144)
tile_x = 0
tile_y = 144
# Checkpoint: 4 frames (2 down + 2 up)
add_frames("checkpoint_flag_down", 2, tile_x, tile_y, 16, 16)
tile_x += 32
add_frames("checkpoint_flag_up", 2, tile_x, tile_y, 16, 16)
tile_x += 32

# Finish: 2 frames
add_frames("finish_gate", 2, tile_x, tile_y, 16, 16)
tile_x += 32

# Ladder
SPRITES.append(("ladder", tile_x, tile_y, 16, 16))
tile_x += 16

# Breakable wall: 2 variants
add_frames("breakable_wall", 2, tile_x, tile_y, 16, 16)
tile_x += 32

# Torch: 4 frames
add_frames("torch", 4, tile_x, tile_y, 16, 16)
tile_x += 64

# Stained glass
SPRITES.append(("stained_glass", tile_x, tile_y, 16, 16))

# --- Row 12-15 (Y 192-255): Power-ups + HUD ---
pu_x = 0
pu_y = 192
for name in [
    "powerup_holy_water",
    "powerup_crucifix",
    "powerup_speed_boots",
    "powerup_double_jump",
    "powerup_armor",
    "powerup_invincibility",
    "powerup_whip_extend",
]:
    SPRITES.append((name, pu_x, pu_y, 16, 16))
    pu_x += 16

# HUD hearts
SPRITES.append(("heart_full", 0, 208, 16, 16))
SPRITES.append(("heart_empty", 16, 208, 16, 16))

# Props
prop_x = 32
prop_y = 208
for name in [
    "prop_candelabra",
    "prop_cross",
    "prop_gravestone",
]:
    SPRITES.append((name, prop_x, prop_y, 16, 16))
    prop_x += 16

# --- Row 16-23 (Y 256-383): Particle sprites ---
part_x = 0
part_y = 256
add_frames("particle_dust", 4, part_x, part_y, 8, 8)
part_x += 32
add_frames("particle_spark", 3, part_x, part_y, 8, 8)
part_x += 24
add_frames("particle_blood", 3, part_x, part_y, 8, 8)
part_x += 24
add_frames("particle_fire", 4, part_x, part_y, 8, 8)
part_x += 32
add_frames("particle_magic", 3, part_x, part_y, 8, 8)
part_x += 24
add_frames("particle_smoke", 3, part_x, part_y, 8, 8)
part_x += 24
add_frames("particle_debris", 3, part_x, part_y, 8, 8)


# Color palette for placeholder generation
COLORS = {
    "player": (80, 120, 200),      # Blue
    "skeleton": (200, 200, 190),    # Bone white
    "bat": (100, 60, 120),          # Purple
    "knight": (150, 150, 160),      # Steel gray
    "medusa": (80, 160, 80),        # Green
    "projectile": (200, 80, 200),   # Magenta
    "stone_brick": (100, 90, 80),   # Dark stone
    "platform": (120, 100, 70),     # Wood brown
    "spikes": (180, 50, 50),        # Red
    "checkpoint": (220, 200, 60),   # Gold
    "finish": (60, 200, 220),       # Cyan
    "ladder": (140, 100, 60),       # Brown
    "breakable": (130, 110, 90),    # Cracked stone
    "torch": (240, 180, 50),        # Fire orange
    "stained_glass": (160, 80, 200),  # Purple glass
    "powerup": (50, 220, 100),      # Green glow
    "heart_full": (220, 40, 40),    # Red
    "heart_empty": (80, 80, 80),    # Gray
    "prop": (160, 140, 120),        # Stone prop
    "particle": (255, 255, 200),    # Light yellow
}


def get_color(name):
    """Get color for a sprite by matching prefix."""
    for prefix, color in COLORS.items():
        if name.startswith(prefix) or prefix in name:
            return color
    return (200, 200, 200)


def draw_placeholder(draw, x, y, w, h, color, name):
    """Draw a distinct placeholder sprite with outlines and variation."""
    # Fill with color
    draw.rectangle([x, y, x + w - 1, y + h - 1], fill=color + (255,))

    # Darker border
    border = tuple(max(0, c - 60) for c in color) + (255,)
    draw.rectangle([x, y, x + w - 1, y + h - 1], outline=border)

    # Add character feature hints for humanoid sprites
    if h == 32 and any(p in name for p in ["player", "skeleton", "knight", "medusa", "bat"]):
        # Head area highlight
        head_color = tuple(min(255, c + 40) for c in color) + (255,)
        draw.rectangle([x + 4, y + 2, x + 11, y + 8], fill=head_color)
        # Eye dots
        draw.point((x + 6, y + 4), fill=(0, 0, 0, 255))
        draw.point((x + 9, y + 4), fill=(0, 0, 0, 255))

    # Add frame number indicators for animations
    for i in range(10):
        if name.endswith(f"_{i}"):
            # Small dot in corner indicating frame number
            dot_x = x + 1 + (i % 4) * 2
            dot_y = y + h - 3
            draw.point((dot_x, dot_y), fill=(255, 255, 255, 200))


def build_atlas():
    """Build the 512x512 sprite atlas."""
    atlas = Image.new("RGBA", (ATLAS_SIZE, ATLAS_SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(atlas)
    metadata = {}

    for name, x, y, w, h in SPRITES:
        color = get_color(name)
        draw_placeholder(draw, x, y, w, h, color, name)
        metadata[name] = {"x": x, "y": y, "w": w, "h": h}

    # Also add legacy names that map to frame 0 for backward compatibility
    legacy_map = {
        "player_idle_0": "player_idle_0",
        "player_walk_0": "player_walk_0",
        "player_jump": "player_jump_0",
        "player_fall": "player_fall_0",
        "player_attack_0": "player_attack_0",
        "player_hurt": "player_hurt_0",
        "player_dead": "player_dead_0",
        "skeleton_walk_0": "skeleton_walk_0",
        "skeleton_walk_1": "skeleton_walk_1",
        "bat_fly_0": "bat_fly_0",
        "bat_fly_1": "bat_fly_1",
        "knight_walk_0": "knight_walk_0",
        "knight_walk_1": "knight_walk_1",
        "medusa_float_0": "medusa_float_0",
        "medusa_float_1": "medusa_float_1",
        "projectile": "projectile_0",
        "stone_brick": "stone_brick_top",
        "checkpoint_flag": "checkpoint_flag_down_0",
        "finish_gate": "finish_gate_0",
        "breakable_wall": "breakable_wall_0",
        "torch": "torch_0",
    }

    os.makedirs(OUT_DIR, exist_ok=True)
    atlas_path = os.path.join(OUT_DIR, "platformer_atlas.png")
    atlas.save(atlas_path, "PNG")
    print(f"Saved atlas: {atlas_path} ({atlas.size[0]}x{atlas.size[1]})")

    json_path = os.path.join(OUT_DIR, "platformer_atlas.json")
    output = {"atlas_size": ATLAS_SIZE, "sprites": metadata, "legacy_aliases": legacy_map}
    with open(json_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"Saved metadata: {json_path} ({len(metadata)} sprites)")

    return atlas, metadata


def build_background():
    """Build the 512x512 parallax background (3 layers, ~170px each stacked vertically)."""
    bg = Image.new("RGBA", (ATLAS_SIZE, ATLAS_SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(bg)

    layer_h = ATLAS_SIZE // 3  # ~170px per layer

    # Layer 0 (sky): dark purple gradient
    for y in range(layer_h):
        t = y / layer_h
        r = int(20 + t * 15)
        g = int(10 + t * 20)
        b = int(40 + t * 30)
        draw.line([(0, y), (ATLAS_SIZE - 1, y)], fill=(r, g, b, 255))
    # Add stars
    import random
    random.seed(42)
    for _ in range(40):
        sx = random.randint(0, ATLAS_SIZE - 1)
        sy = random.randint(0, layer_h - 1)
        brightness = random.randint(150, 255)
        draw.point((sx, sy), fill=(brightness, brightness, brightness, 255))

    # Layer 1 (mid-ground): dark silhouette hills
    y_base = layer_h
    for y in range(layer_h):
        t = y / layer_h
        r = int(30 + t * 20)
        g = int(25 + t * 15)
        b = int(50 + t * 20)
        draw.line([(0, y_base + y), (ATLAS_SIZE - 1, y_base + y)], fill=(r, g, b, 255))
    # Silhouette hills
    import math
    for x in range(ATLAS_SIZE):
        hill_h = int(20 + 15 * math.sin(x * 0.02) + 10 * math.sin(x * 0.05 + 1.0))
        for y in range(hill_h):
            py = y_base + layer_h - 1 - y
            if 0 <= py < y_base + layer_h:
                draw.point((x, py), fill=(20, 15, 35, 255))

    # Layer 2 (near-ground): darker buildings/trees silhouette
    y_base = layer_h * 2
    for y in range(layer_h):
        t = y / layer_h
        r = int(40 + t * 25)
        g = int(30 + t * 20)
        b = int(55 + t * 25)
        draw.line([(0, y_base + y), (ATLAS_SIZE - 1, y_base + y)], fill=(r, g, b, 255))
    # Building silhouettes
    for bx in range(0, ATLAS_SIZE, 32):
        bh = random.randint(30, 60)
        bw = random.randint(20, 30)
        for x in range(bx, min(bx + bw, ATLAS_SIZE)):
            for y in range(bh):
                py = y_base + layer_h - 1 - y
                if 0 <= py < y_base + layer_h:
                    draw.point((x, py), fill=(15, 10, 25, 255))
        # Window lights
        for wx in range(bx + 3, min(bx + bw - 3, ATLAS_SIZE), 6):
            for wy in range(5, bh - 5, 8):
                if random.random() > 0.4:
                    py = y_base + layer_h - 1 - wy
                    if 0 <= py < y_base + layer_h:
                        draw.rectangle(
                            [wx, py - 2, wx + 2, py],
                            fill=(200, 180, 80, 180),
                        )

    bg_path = os.path.join(OUT_DIR, "platformer_bg.png")
    bg.save(bg_path, "PNG")
    print(f"Saved background: {bg_path} ({bg.size[0]}x{bg.size[1]})")


if __name__ == "__main__":
    build_atlas()
    build_background()
    print("Done!")
