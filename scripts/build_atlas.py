#!/usr/bin/env python3
"""
Build the 1024x512 platformer sprite atlas and parallax background texture.

Generates GothicVania-style pixel art sprites with distinct silhouettes and
detail for each character type, tile, and effect. Outputs:
  - web/assets/sprites/platformer_atlas.png  (1024x512 RGBA)
  - web/assets/sprites/platformer_atlas.json (sprite name -> rect mapping)
  - web/assets/sprites/platformer_bg.png     (512x512, 3 layers stacked)

Usage:
    python scripts/build_atlas.py
"""

import json
import math
import os
import random
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:
    print("Pillow is required: pip install Pillow", file=sys.stderr)
    sys.exit(1)

ATLAS_W = 1024
ATLAS_H = 512
CELL = 16  # Base cell size
OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "web", "assets", "sprites")

# Sprite definitions: (name, x, y, w, h)
SPRITES = []


def add_frames(prefix, count, x_start, y, w, h):
    """Add numbered animation frames."""
    for i in range(count):
        SPRITES.append((f"{prefix}_{i}", x_start + i * w, y, w, h))


# --- Row 0-3 (Y 0-63): Player sprites (16x32 each) ---
add_frames("player_idle", 6, 0, 0, 16, 32)
add_frames("player_walk", 6, 96, 0, 16, 32)
add_frames("player_jump", 3, 192, 0, 16, 32)
add_frames("player_fall", 3, 240, 0, 16, 32)
add_frames("player_attack", 6, 288, 0, 16, 32)
add_frames("player_hurt", 2, 384, 0, 16, 32)
add_frames("player_dead", 4, 416, 0, 16, 32)

# --- Row 4-7 (Y 64-127): Enemy sprites (16x32 each) ---
add_frames("skeleton_walk", 4, 0, 64, 16, 32)
add_frames("skeleton_attack", 3, 64, 64, 16, 32)
add_frames("skeleton_death", 4, 112, 64, 16, 32)
add_frames("bat_fly", 4, 176, 64, 16, 32)
add_frames("bat_death", 2, 240, 64, 16, 32)
add_frames("knight_walk", 4, 272, 64, 16, 32)
add_frames("knight_attack", 3, 336, 64, 16, 32)
add_frames("knight_death", 4, 384, 64, 16, 32)
add_frames("medusa_float", 4, 0, 96, 16, 32)
add_frames("medusa_death", 2, 64, 96, 16, 32)
add_frames("projectile", 3, 96, 96, 16, 16)

# --- Row 8-11 (Y 128-191): Tiles (16x16 each) ---
tile_x = 0
tile_y = 128
for name in [
    "stone_brick_top", "stone_brick_inner", "stone_brick_left",
    "stone_brick_right", "stone_brick_top_left", "stone_brick_top_right",
    "stone_brick_bottom_left", "stone_brick_bottom_right",
]:
    SPRITES.append((name, tile_x, tile_y, 16, 16))
    tile_x += 16

add_frames("platform", 3, tile_x, tile_y, 16, 16)
tile_x += 48
add_frames("spikes", 2, tile_x, tile_y, 16, 16)
tile_x += 32

tile_x = 0
tile_y = 144
add_frames("checkpoint_flag_down", 2, tile_x, tile_y, 16, 16)
tile_x += 32
add_frames("checkpoint_flag_up", 2, tile_x, tile_y, 16, 16)
tile_x += 32
add_frames("finish_gate", 2, tile_x, tile_y, 16, 16)
tile_x += 32
SPRITES.append(("ladder", tile_x, tile_y, 16, 16))
tile_x += 16
add_frames("breakable_wall", 2, tile_x, tile_y, 16, 16)
tile_x += 32
add_frames("torch", 4, tile_x, tile_y, 16, 16)
tile_x += 64
SPRITES.append(("stained_glass", tile_x, tile_y, 16, 16))

# Water tiles (new for Phase 2)
SPRITES.append(("water_surface", 224, 144, 16, 16))
SPRITES.append(("water_body", 240, 144, 16, 16))

# Decorative tiles (Phase 6)
SPRITES.append(("cobweb", 256, 144, 16, 16))
SPRITES.append(("chain_0", 272, 144, 16, 16))
SPRITES.append(("chain_1", 288, 144, 16, 16))

# --- Row 12-15 (Y 192-255): Power-ups + HUD ---
pu_x = 0
pu_y = 192
for name in [
    "powerup_holy_water", "powerup_crucifix", "powerup_speed_boots",
    "powerup_double_jump", "powerup_armor", "powerup_invincibility",
    "powerup_whip_extend",
]:
    SPRITES.append((name, pu_x, pu_y, 16, 16))
    pu_x += 16

SPRITES.append(("heart_full", 0, 208, 16, 16))
SPRITES.append(("heart_empty", 16, 208, 16, 16))

prop_x = 32
prop_y = 208
for name in ["prop_candelabra", "prop_cross", "prop_gravestone"]:
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
# Additional particle sprites (Phase 8)
part_x += 24
add_frames("particle_water", 3, part_x, part_y, 8, 8)
part_x += 24
add_frames("particle_ember", 3, part_x, part_y, 8, 8)

# ═══════════════════════════════════════════════════════════════════
# GothicVania-style pixel art generation
# ═══════════════════════════════════════════════════════════════════

# Gothic color palette
PAL = {
    "skin": (220, 190, 160),
    "skin_shadow": (180, 150, 120),
    "hair_dark": (40, 30, 50),
    "cape_red": (160, 40, 40),
    "cape_shadow": (100, 25, 30),
    "armor_steel": (160, 160, 170),
    "armor_shadow": (100, 100, 115),
    "cloth_dark": (50, 40, 60),
    "cloth_mid": (80, 60, 90),
    "boots": (60, 40, 30),
    "bone": (220, 210, 190),
    "bone_shadow": (170, 160, 140),
    "bone_dark": (130, 120, 100),
    "bat_purple": (90, 50, 110),
    "bat_wing": (120, 70, 140),
    "knight_steel": (140, 145, 160),
    "knight_shadow": (90, 95, 110),
    "knight_gold": (200, 170, 60),
    "medusa_green": (60, 140, 80),
    "medusa_scale": (40, 100, 60),
    "medusa_hair": (80, 160, 100),
    "stone_light": (120, 110, 100),
    "stone_mid": (90, 82, 75),
    "stone_dark": (65, 58, 52),
    "stone_mortar": (75, 70, 62),
    "wood_light": (140, 105, 65),
    "wood_mid": (110, 80, 50),
    "wood_dark": (80, 55, 35),
    "spike_metal": (140, 50, 50),
    "spike_tip": (200, 80, 80),
    "gold": (240, 200, 60),
    "gold_shadow": (200, 160, 40),
    "cyan": (60, 200, 220),
    "cyan_shadow": (40, 150, 170),
    "fire_bright": (255, 220, 80),
    "fire_mid": (240, 160, 40),
    "fire_dark": (200, 80, 20),
    "glass_purple": (160, 80, 180),
    "glass_blue": (80, 120, 200),
    "glass_lead": (60, 60, 70),
    "water_surface": (60, 140, 200),
    "water_body": (40, 90, 160),
    "water_highlight": (100, 180, 240),
}


def px(draw, x, y, color, alpha=255):
    """Draw a single pixel with optional alpha."""
    if alpha < 255:
        draw.point((x, y), fill=color + (alpha,))
    else:
        draw.point((x, y), fill=color + (255,))


def rect(draw, x, y, w, h, color, alpha=255):
    """Draw a filled rectangle."""
    c = color + (alpha,)
    draw.rectangle([x, y, x + w - 1, y + h - 1], fill=c)


def outline_rect(draw, x, y, w, h, color):
    """Draw an outlined rectangle."""
    draw.rectangle([x, y, x + w - 1, y + h - 1], outline=color + (255,))


# ── Player sprite drawing ──────────────────────────────────────────

def draw_player_idle(draw, bx, by, frame):
    """Castlevania hero: cape, whip at side, standing pose."""
    bob = [0, 0, -1, -1, 0, 0][frame % 6]
    y = by + bob
    # Boots
    rect(draw, bx + 4, y + 26, 3, 4, PAL["boots"])
    rect(draw, bx + 9, y + 26, 3, 4, PAL["boots"])
    # Legs
    rect(draw, bx + 5, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 9, y + 22, 2, 5, PAL["cloth_dark"])
    # Torso
    rect(draw, bx + 4, y + 14, 8, 8, PAL["cloth_mid"])
    rect(draw, bx + 5, y + 15, 6, 6, PAL["cloth_dark"])
    # Cape (behind)
    rect(draw, bx + 2, y + 12, 3, 14, PAL["cape_red"])
    rect(draw, bx + 2, y + 14, 2, 12, PAL["cape_shadow"])
    # Belt
    rect(draw, bx + 4, y + 21, 8, 1, PAL["gold_shadow"])
    # Arms
    rect(draw, bx + 3, y + 15, 2, 6, PAL["skin"])
    rect(draw, bx + 11, y + 15, 2, 6, PAL["skin"])
    # Head
    rect(draw, bx + 5, y + 6, 6, 8, PAL["skin"])
    rect(draw, bx + 5, y + 7, 6, 6, PAL["skin_shadow"])
    # Hair
    rect(draw, bx + 4, y + 4, 8, 4, PAL["hair_dark"])
    rect(draw, bx + 4, y + 6, 2, 6, PAL["hair_dark"])
    # Eyes
    px(draw, bx + 7, y + 9, (255, 255, 255))
    px(draw, bx + 10, y + 9, (255, 255, 255))
    px(draw, bx + 8, y + 9, (30, 30, 60))
    px(draw, bx + 11, y + 9, (30, 30, 60))


def draw_player_walk(draw, bx, by, frame):
    """Walking animation with leg alternation."""
    leg_offset = [0, 1, 2, 1, 0, -1][frame % 6]
    y = by
    # Boots (alternating)
    rect(draw, bx + 4 + leg_offset, y + 26, 3, 4, PAL["boots"])
    rect(draw, bx + 9 - leg_offset, y + 26, 3, 4, PAL["boots"])
    # Legs
    rect(draw, bx + 5 + leg_offset, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 9 - leg_offset, y + 22, 2, 5, PAL["cloth_dark"])
    # Body (same as idle)
    rect(draw, bx + 4, y + 14, 8, 8, PAL["cloth_mid"])
    rect(draw, bx + 5, y + 15, 6, 6, PAL["cloth_dark"])
    # Cape billowing
    cape_w = 3 + abs(leg_offset)
    rect(draw, bx + 1, y + 12, cape_w, 14, PAL["cape_red"])
    rect(draw, bx + 1, y + 14, cape_w - 1, 12, PAL["cape_shadow"])
    rect(draw, bx + 4, y + 21, 8, 1, PAL["gold_shadow"])
    rect(draw, bx + 3, y + 15, 2, 6, PAL["skin"])
    rect(draw, bx + 11, y + 15, 2, 5, PAL["skin"])
    # Head
    rect(draw, bx + 5, y + 6, 6, 8, PAL["skin"])
    rect(draw, bx + 4, y + 4, 8, 4, PAL["hair_dark"])
    rect(draw, bx + 4, y + 6, 2, 6, PAL["hair_dark"])
    px(draw, bx + 7, y + 9, (255, 255, 255))
    px(draw, bx + 10, y + 9, (255, 255, 255))
    px(draw, bx + 8, y + 9, (30, 30, 60))
    px(draw, bx + 11, y + 9, (30, 30, 60))


def draw_player_jump(draw, bx, by, frame):
    """Jump: stretched pose, arms up."""
    y = by + [-1, -2, -1][frame % 3]
    rect(draw, bx + 5, y + 26, 2, 4, PAL["boots"])
    rect(draw, bx + 9, y + 26, 2, 4, PAL["boots"])
    rect(draw, bx + 5, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 9, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 4, y + 14, 8, 8, PAL["cloth_mid"])
    rect(draw, bx + 2, y + 14, 3, 10, PAL["cape_red"])
    rect(draw, bx + 4, y + 21, 8, 1, PAL["gold_shadow"])
    # Arms raised
    rect(draw, bx + 3, y + 12, 2, 4, PAL["skin"])
    rect(draw, bx + 11, y + 12, 2, 4, PAL["skin"])
    rect(draw, bx + 5, y + 6, 6, 8, PAL["skin"])
    rect(draw, bx + 4, y + 4, 8, 4, PAL["hair_dark"])
    rect(draw, bx + 4, y + 6, 2, 6, PAL["hair_dark"])
    px(draw, bx + 7, y + 9, (255, 255, 255))
    px(draw, bx + 10, y + 9, (255, 255, 255))


def draw_player_fall(draw, bx, by, frame):
    """Falling: legs spread, cape billowing up."""
    y = by + [0, 1, 0][frame % 3]
    rect(draw, bx + 3, y + 26, 3, 4, PAL["boots"])
    rect(draw, bx + 10, y + 26, 3, 4, PAL["boots"])
    rect(draw, bx + 4, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 10, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 4, y + 14, 8, 8, PAL["cloth_mid"])
    # Cape billowing up
    rect(draw, bx + 1, y + 10, 4, 12, PAL["cape_red"])
    rect(draw, bx + 1, y + 8, 3, 6, PAL["cape_shadow"])
    rect(draw, bx + 4, y + 21, 8, 1, PAL["gold_shadow"])
    rect(draw, bx + 3, y + 16, 2, 5, PAL["skin"])
    rect(draw, bx + 11, y + 16, 2, 5, PAL["skin"])
    rect(draw, bx + 5, y + 6, 6, 8, PAL["skin"])
    rect(draw, bx + 4, y + 4, 8, 4, PAL["hair_dark"])
    px(draw, bx + 7, y + 9, (255, 255, 255))
    px(draw, bx + 10, y + 9, (255, 255, 255))


def draw_player_attack(draw, bx, by, frame):
    """Whip attack: arm extended with whip arc."""
    y = by
    rect(draw, bx + 5, y + 26, 2, 4, PAL["boots"])
    rect(draw, bx + 9, y + 26, 2, 4, PAL["boots"])
    rect(draw, bx + 5, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 9, y + 22, 2, 5, PAL["cloth_dark"])
    rect(draw, bx + 4, y + 14, 8, 8, PAL["cloth_mid"])
    rect(draw, bx + 2, y + 14, 3, 10, PAL["cape_red"])
    rect(draw, bx + 4, y + 21, 8, 1, PAL["gold_shadow"])
    # Extended arm + whip
    arm_ext = min(frame, 4)
    rect(draw, bx + 11, y + 15, 2 + arm_ext, 2, PAL["skin"])
    # Whip
    whip_len = min(frame * 2, 8)
    if whip_len > 0:
        for i in range(whip_len):
            wx = bx + 13 + arm_ext + i
            wy = y + 14 + (i * i) // 6
            if wx < bx + 16 and wy < by + 32:
                px(draw, wx, wy, PAL["wood_light"])
    rect(draw, bx + 3, y + 16, 2, 5, PAL["skin"])
    rect(draw, bx + 5, y + 6, 6, 8, PAL["skin"])
    rect(draw, bx + 4, y + 4, 8, 4, PAL["hair_dark"])
    rect(draw, bx + 4, y + 6, 2, 6, PAL["hair_dark"])
    px(draw, bx + 7, y + 9, (255, 255, 255))
    px(draw, bx + 10, y + 9, (255, 255, 255))


def draw_player_hurt(draw, bx, by, frame):
    """Hurt: recoil pose, red flash."""
    y = by + [1, -1][frame % 2]
    rect(draw, bx + 6, y + 26, 2, 4, PAL["boots"])
    rect(draw, bx + 8, y + 26, 2, 4, PAL["boots"])
    rect(draw, bx + 6, y + 22, 4, 5, PAL["cloth_dark"])
    rect(draw, bx + 4, y + 14, 8, 8, PAL["cloth_mid"])
    rect(draw, bx + 2, y + 14, 3, 10, PAL["cape_red"])
    rect(draw, bx + 4, y + 21, 8, 1, PAL["gold_shadow"])
    rect(draw, bx + 3, y + 16, 2, 5, (220, 160, 160))
    rect(draw, bx + 11, y + 16, 2, 5, (220, 160, 160))
    rect(draw, bx + 5, y + 6, 6, 8, (220, 180, 170))
    rect(draw, bx + 4, y + 4, 8, 4, PAL["hair_dark"])
    px(draw, bx + 7, y + 9, (255, 100, 100))
    px(draw, bx + 10, y + 9, (255, 100, 100))


def draw_player_dead(draw, bx, by, frame):
    """Dead: falling over, then lying flat."""
    tilt = min(frame, 3)
    y = by
    if tilt < 2:
        # Falling over
        rect(draw, bx + 5, y + 26, 6, 4, PAL["boots"])
        rect(draw, bx + 4, y + 14 + tilt * 3, 8, 12 - tilt * 3, PAL["cloth_dark"])
        rect(draw, bx + 2, y + 16 + tilt * 2, 3, 8, PAL["cape_red"])
        rect(draw, bx + 5, y + 6 + tilt * 2, 6, 8, PAL["skin"])
        rect(draw, bx + 4, y + 4 + tilt * 2, 8, 4, PAL["hair_dark"])
    else:
        # Lying flat on ground
        rect(draw, bx + 2, y + 26, 12, 4, PAL["cape_red"])
        rect(draw, bx + 3, y + 24, 10, 3, PAL["cloth_dark"])
        rect(draw, bx + 3, y + 22, 6, 3, PAL["skin"])
        rect(draw, bx + 2, y + 21, 8, 2, PAL["hair_dark"])


# ── Enemy sprite drawing ───────────────────────────────────────────

def draw_skeleton(draw, bx, by, frame, action="walk"):
    """Skeleton enemy: bone-colored, visible ribcage."""
    bob = [0, -1, 0, 1][frame % 4] if action == "walk" else 0
    y = by + bob
    if action == "death":
        # Crumbling apart
        scatter = frame * 2
        for i in range(6):
            ox = random.Random(frame * 10 + i).randint(-scatter, scatter)
            oy = random.Random(frame * 10 + i + 50).randint(-scatter, scatter)
            rect(draw, bx + 6 + ox, y + 12 + i * 3 + oy, 3, 2, PAL["bone"])
        return
    # Feet
    rect(draw, bx + 5, y + 28, 2, 2, PAL["bone_dark"])
    rect(draw, bx + 9, y + 28, 2, 2, PAL["bone_dark"])
    # Legs (thin bones)
    px(draw, bx + 6, y + 24, PAL["bone"])
    px(draw, bx + 6, y + 25, PAL["bone"])
    px(draw, bx + 6, y + 26, PAL["bone"])
    px(draw, bx + 10, y + 24, PAL["bone"])
    px(draw, bx + 10, y + 25, PAL["bone"])
    px(draw, bx + 10, y + 26, PAL["bone"])
    # Ribcage
    rect(draw, bx + 5, y + 16, 6, 8, PAL["bone_shadow"])
    for rib_y in range(3):
        px(draw, bx + 6, y + 17 + rib_y * 2, PAL["bone"])
        px(draw, bx + 9, y + 17 + rib_y * 2, PAL["bone"])
    # Skull
    rect(draw, bx + 5, y + 8, 6, 8, PAL["bone"])
    rect(draw, bx + 6, y + 9, 4, 6, PAL["bone_shadow"])
    # Eye sockets
    px(draw, bx + 6, y + 11, (20, 10, 10))
    px(draw, bx + 9, y + 11, (20, 10, 10))
    # Red eye glow
    px(draw, bx + 6, y + 11, (180, 30, 30))
    px(draw, bx + 9, y + 11, (180, 30, 30))
    # Arms
    if action == "attack":
        rect(draw, bx + 11, y + 17, 4, 1, PAL["bone"])
        rect(draw, bx + 14, y + 15, 1, 3, PAL["bone"])  # raised arm
    else:
        rect(draw, bx + 3, y + 17, 2, 5, PAL["bone_shadow"])
        rect(draw, bx + 11, y + 17, 2, 5, PAL["bone_shadow"])


def draw_bat(draw, bx, by, frame, alive=True):
    """Bat: dark purple body with spread wings."""
    if not alive:
        # Falling with folded wings
        alpha = max(0, 180 - frame * 60)
        rect(draw, bx + 6, by + 16, 4, 6, PAL["bat_purple"], alpha)
        return
    y = by + [0, -2, -1, 1][frame % 4]
    wing_spread = [4, 6, 5, 3][frame % 4]
    # Body
    rect(draw, bx + 6, y + 14, 4, 6, PAL["bat_purple"])
    # Wings
    rect(draw, bx + 6 - wing_spread, y + 14, wing_spread, 3, PAL["bat_wing"])
    rect(draw, bx + 10, y + 14, wing_spread, 3, PAL["bat_wing"])
    # Wing membrane detail
    for i in range(wing_spread):
        px(draw, bx + 6 - wing_spread + i, y + 16, PAL["bat_purple"], 200)
        px(draw, bx + 10 + i, y + 16, PAL["bat_purple"], 200)
    # Eyes
    px(draw, bx + 7, y + 15, (255, 60, 60))
    px(draw, bx + 9, y + 15, (255, 60, 60))
    # Ears
    px(draw, bx + 6, y + 13, PAL["bat_purple"])
    px(draw, bx + 9, y + 13, PAL["bat_purple"])


def draw_knight(draw, bx, by, frame, action="walk"):
    """Armored knight: heavy steel plate, sword."""
    bob = [0, 0, -1, 0][frame % 4] if action == "walk" else 0
    y = by + bob
    if action == "death":
        scatter = frame * 2
        for i in range(5):
            ox = random.Random(frame * 7 + i).randint(-scatter, scatter)
            oy = random.Random(frame * 7 + i + 30).randint(0, scatter)
            rect(draw, bx + 5 + ox, y + 14 + i * 3 + oy, 4, 3, PAL["knight_steel"])
        return
    # Boots
    rect(draw, bx + 4, y + 27, 3, 3, PAL["knight_shadow"])
    rect(draw, bx + 9, y + 27, 3, 3, PAL["knight_shadow"])
    # Leg armor
    rect(draw, bx + 5, y + 22, 2, 6, PAL["knight_steel"])
    rect(draw, bx + 9, y + 22, 2, 6, PAL["knight_steel"])
    # Chest plate
    rect(draw, bx + 4, y + 14, 8, 8, PAL["knight_steel"])
    rect(draw, bx + 5, y + 15, 6, 6, PAL["knight_shadow"])
    # Gold trim
    rect(draw, bx + 4, y + 14, 8, 1, PAL["knight_gold"])
    rect(draw, bx + 4, y + 21, 8, 1, PAL["knight_gold"])
    # Helmet
    rect(draw, bx + 4, y + 6, 8, 8, PAL["knight_steel"])
    rect(draw, bx + 5, y + 7, 6, 6, PAL["knight_shadow"])
    # Visor slit
    rect(draw, bx + 6, y + 10, 4, 1, (20, 20, 30))
    # Plume
    rect(draw, bx + 6, y + 4, 4, 3, PAL["cape_red"])
    # Shield arm
    rect(draw, bx + 2, y + 15, 3, 7, PAL["knight_steel"])
    rect(draw, bx + 2, y + 16, 2, 5, PAL["knight_shadow"])
    # Sword arm
    if action == "attack":
        rect(draw, bx + 11, y + 12, 2, 2, PAL["skin"])
        rect(draw, bx + 12, y + 8, 1, 6, (200, 200, 210))  # raised sword
        px(draw, bx + 12, y + 7, (240, 240, 255))  # sword tip
    else:
        rect(draw, bx + 11, y + 15, 2, 6, PAL["skin"])
        rect(draw, bx + 12, y + 20, 1, 6, (200, 200, 210))  # sword down


def draw_medusa(draw, bx, by, frame, alive=True):
    """Medusa: serpentine body, snake hair, floating."""
    if not alive:
        alpha = max(0, 200 - frame * 80)
        rect(draw, bx + 4, by + 14, 8, 10, PAL["medusa_green"], alpha)
        return
    y = by + [0, -1, -2, -1][frame % 4]
    # Serpentine lower body (tail)
    wave = [0, 1, 0, -1][frame % 4]
    rect(draw, bx + 5 + wave, y + 22, 6, 8, PAL["medusa_scale"])
    rect(draw, bx + 6 + wave, y + 28, 4, 2, PAL["medusa_green"])
    # Scale pattern
    for i in range(3):
        px(draw, bx + 6 + wave, y + 23 + i * 2, PAL["medusa_green"])
        px(draw, bx + 9 + wave, y + 23 + i * 2, PAL["medusa_green"])
    # Torso
    rect(draw, bx + 5, y + 14, 6, 8, PAL["medusa_green"])
    # Face
    rect(draw, bx + 5, y + 8, 6, 6, PAL["skin"])
    # Snake hair
    for i in range(4):
        sx = bx + 4 + i * 2
        sy_offset = [0, -1, 0, 1][(frame + i) % 4]
        px(draw, sx, y + 5 + sy_offset, PAL["medusa_hair"])
        px(draw, sx, y + 4 + sy_offset, PAL["medusa_hair"])
        px(draw, sx + 1, y + 3 + sy_offset, PAL["medusa_hair"])
    # Eyes (glowing)
    px(draw, bx + 6, y + 10, (200, 255, 100))
    px(draw, bx + 9, y + 10, (200, 255, 100))
    # Arms
    rect(draw, bx + 3, y + 15, 2, 4, PAL["skin"])
    rect(draw, bx + 11, y + 15, 2, 4, PAL["skin"])


# ── Tile drawing ───────────────────────────────────────────────────

def draw_stone_brick(draw, bx, by, variant):
    """Draw detailed stone brick with mortar lines."""
    rect(draw, bx, by, 16, 16, PAL["stone_mid"])
    # Mortar lines (horizontal)
    rect(draw, bx, by + 7, 16, 1, PAL["stone_mortar"])
    rect(draw, bx, by + 15, 16, 1, PAL["stone_mortar"])
    # Mortar lines (vertical, offset per row)
    rect(draw, bx + 7, by, 1, 8, PAL["stone_mortar"])
    rect(draw, bx + 3, by + 8, 1, 8, PAL["stone_mortar"])
    rect(draw, bx + 11, by + 8, 1, 8, PAL["stone_mortar"])
    # Highlights and shadows for depth
    rect(draw, bx + 1, by + 1, 5, 1, PAL["stone_light"])
    rect(draw, bx + 9, by + 1, 5, 1, PAL["stone_light"])
    rect(draw, bx + 1, by + 9, 5, 1, PAL["stone_light"])
    # Shadow on bottom edges
    rect(draw, bx + 1, by + 6, 5, 1, PAL["stone_dark"])
    rect(draw, bx + 9, by + 6, 5, 1, PAL["stone_dark"])
    # Variant-specific edge highlighting
    if "top" in variant and "left" not in variant and "right" not in variant:
        rect(draw, bx, by, 16, 2, PAL["stone_light"])
        rect(draw, bx, by, 16, 1, (140, 130, 120))
    if "left" in variant:
        rect(draw, bx, by, 2, 16, PAL["stone_light"])
    if "right" in variant:
        rect(draw, bx + 14, by, 2, 16, PAL["stone_dark"])


def draw_platform(draw, bx, by, variant):
    """Wooden platform with plank details."""
    rect(draw, bx, by, 16, 16, PAL["wood_mid"])
    # Top planks
    rect(draw, bx, by, 16, 4, PAL["wood_light"])
    rect(draw, bx, by, 16, 1, (160, 120, 75))
    # Plank lines
    rect(draw, bx + 5, by, 1, 4, PAL["wood_dark"])
    rect(draw, bx + 11, by, 1, 4, PAL["wood_dark"])
    # Support beams underneath
    rect(draw, bx + 2, by + 5, 2, 11, PAL["wood_dark"])
    rect(draw, bx + 12, by + 5, 2, 11, PAL["wood_dark"])
    # Cross brace
    for i in range(8):
        px(draw, bx + 4 + i, by + 7 + i, PAL["wood_dark"])


def draw_spikes(draw, bx, by, variant):
    """Metal spikes pointing upward."""
    rect(draw, bx, by + 12, 16, 4, PAL["stone_dark"])
    for i in range(4):
        sx = bx + i * 4
        # Spike triangle
        for row in range(10):
            w = 3 - (row * 3) // 10
            if w > 0:
                x_off = (3 - w) // 2
                c = PAL["spike_tip"] if row < 3 else PAL["spike_metal"]
                rect(draw, sx + 1 + x_off, by + 12 - row, w, 1, c)


def draw_torch(draw, bx, by, frame):
    """Wall torch with animated flame."""
    # Bracket
    rect(draw, bx + 6, by + 8, 4, 8, PAL["stone_dark"])
    rect(draw, bx + 5, by + 8, 6, 2, PAL["stone_light"])
    # Torch stick
    rect(draw, bx + 7, by + 4, 2, 6, PAL["wood_dark"])
    # Flame (animated)
    flame_h = [5, 6, 5, 7][frame % 4]
    flame_w = [3, 4, 3, 4][frame % 4]
    fx = bx + 8 - flame_w // 2
    fy = by + 4 - flame_h
    rect(draw, fx, fy + 2, flame_w, flame_h - 2, PAL["fire_mid"])
    rect(draw, fx + 1, fy, flame_w - 2, flame_h - 1, PAL["fire_bright"])
    px(draw, bx + 7, fy + flame_h - 1, PAL["fire_dark"])
    px(draw, bx + 8, fy + flame_h - 1, PAL["fire_dark"])


def draw_checkpoint_flag(draw, bx, by, frame, activated):
    """Checkpoint flag on a pole."""
    # Pole
    rect(draw, bx + 7, by + 2, 2, 14, PAL["stone_light"])
    # Base
    rect(draw, bx + 4, by + 14, 8, 2, PAL["stone_dark"])
    # Flag
    flag_color = PAL["gold"] if activated else PAL["stone_dark"]
    wave = [0, 1, 0, -1][frame % 4] if activated else 0
    rect(draw, bx + 9, by + 2 + wave, 5, 4, flag_color)
    if activated:
        px(draw, bx + 10, by + 3 + wave, PAL["gold_shadow"])


def draw_finish_gate(draw, bx, by, frame):
    """Ornate finish gate with pulsing glow."""
    # Stone pillars
    rect(draw, bx + 1, by + 2, 4, 14, PAL["stone_light"])
    rect(draw, bx + 11, by + 2, 4, 14, PAL["stone_light"])
    # Arch top
    rect(draw, bx + 1, by + 1, 14, 3, PAL["stone_light"])
    rect(draw, bx + 3, by, 10, 2, PAL["gold"])
    # Gate bars
    for i in range(3):
        rect(draw, bx + 5 + i * 2, by + 3, 1, 13, PAL["knight_gold"])
    # Pulsing gem
    glow = [200, 240, 255, 240][frame % 4]
    px(draw, bx + 7, by + 1, (glow, glow // 2, glow // 4))
    px(draw, bx + 8, by + 1, (glow, glow // 2, glow // 4))


def draw_ladder(draw, bx, by):
    """Wooden ladder."""
    rect(draw, bx + 3, by, 2, 16, PAL["wood_mid"])
    rect(draw, bx + 11, by, 2, 16, PAL["wood_mid"])
    # Rungs
    for i in range(4):
        rect(draw, bx + 3, by + 2 + i * 4, 10, 2, PAL["wood_light"])
        rect(draw, bx + 4, by + 3 + i * 4, 8, 1, PAL["wood_dark"])


def draw_breakable_wall(draw, bx, by, variant):
    """Cracked stone wall."""
    draw_stone_brick(draw, bx, by, "inner")
    # Cracks
    crack_color = PAL["stone_dark"]
    # Diagonal crack
    for i in range(6):
        px(draw, bx + 3 + i, by + 4 + i, crack_color)
    for i in range(4):
        px(draw, bx + 8 + i, by + 2 + i, crack_color)
    # Loose mortar
    if variant == 1:
        px(draw, bx + 5, by + 10, PAL["stone_mortar"])
        px(draw, bx + 10, by + 8, PAL["stone_mortar"])


def draw_stained_glass(draw, bx, by):
    """Gothic stained glass window."""
    # Frame
    outline_rect(draw, bx + 1, by + 1, 14, 14, PAL["glass_lead"])
    # Arch top
    rect(draw, bx + 2, by + 1, 12, 2, PAL["glass_lead"])
    # Glass panels
    rect(draw, bx + 2, by + 3, 6, 5, PAL["glass_purple"])
    rect(draw, bx + 8, by + 3, 6, 5, PAL["glass_blue"])
    rect(draw, bx + 2, by + 9, 6, 5, PAL["glass_blue"])
    rect(draw, bx + 8, by + 9, 6, 5, PAL["glass_purple"])
    # Cross divider
    rect(draw, bx + 7, by + 3, 2, 11, PAL["glass_lead"])
    rect(draw, bx + 2, by + 8, 12, 1, PAL["glass_lead"])
    # Light spot
    px(draw, bx + 5, by + 5, (200, 150, 220), 200)
    px(draw, bx + 10, by + 11, (150, 180, 240), 200)


def draw_water_tile(draw, bx, by, is_surface):
    """Water tile: surface has wave pattern, body is translucent blue."""
    if is_surface:
        # Surface with wave highlights
        rect(draw, bx, by + 4, 16, 12, PAL["water_body"], 180)
        # Wave crests
        for i in range(8):
            wave_y = by + 2 + int(math.sin(i * 0.8) * 2)
            rect(draw, bx + i * 2, wave_y, 2, 3, PAL["water_surface"], 200)
        # Highlights
        px(draw, bx + 3, by + 3, PAL["water_highlight"], 200)
        px(draw, bx + 11, by + 5, PAL["water_highlight"], 180)
    else:
        rect(draw, bx, by, 16, 16, PAL["water_body"], 160)
        # Caustic light patterns
        for i in range(3):
            cx = bx + 3 + i * 5
            cy = by + 4 + i * 4
            px(draw, cx, cy, PAL["water_highlight"], 120)
            px(draw, cx + 1, cy, PAL["water_highlight"], 80)


def draw_cobweb(draw, bx, by):
    """Cobweb decoration: thin strands in a triangular pattern."""
    c = (180, 180, 190)
    # Corner-to-corner strands
    for i in range(14):
        # Top-left to bottom-right diagonal strands
        px(draw, bx + i, by + i, c, 120 - i * 5)
        # Horizontal strands
        if i < 12:
            px(draw, bx + i + 2, by + i // 2 + 1, c, 80)
    # Web connections
    for i in range(5):
        px(draw, bx + 2 + i * 2, by + 3, c, 100)
        px(draw, bx + 4 + i * 2, by + 6, c, 80)
    px(draw, bx, by, c, 160)


def draw_chain(draw, bx, by, frame):
    """Hanging chain link: alternates link orientation per frame."""
    link_color = (140, 130, 120)
    highlight = (180, 170, 160)
    # Vertical chain with alternating link shapes
    for i in range(4):
        cy = by + i * 4
        if (i + frame) % 2 == 0:
            # Horizontal oval link
            rect(draw, bx + 5, cy, 6, 3, link_color, 200)
            px(draw, bx + 6, cy, highlight, 200)
        else:
            # Vertical oval link
            rect(draw, bx + 6, cy, 4, 4, link_color, 200)
            px(draw, bx + 7, cy, highlight, 180)


# ── Power-up and UI drawing ───────────────────────────────────────

def draw_powerup(draw, bx, by, kind):
    """Draw a power-up icon in GothicVania style."""
    # Background glow circle
    for dy in range(-5, 6):
        for dx in range(-5, 6):
            if dx * dx + dy * dy <= 25:
                px(draw, bx + 8 + dx, by + 8 + dy, (40, 80, 40), 60)

    colors = {
        "holy_water": ((80, 140, 220), (40, 80, 160)),
        "crucifix": ((220, 200, 60), (180, 160, 40)),
        "speed_boots": ((60, 200, 100), (30, 140, 60)),
        "double_jump": ((200, 160, 255), (140, 100, 200)),
        "armor": ((180, 180, 190), (120, 120, 140)),
        "invincibility": ((255, 220, 80), (220, 180, 40)),
        "whip_extend": ((200, 120, 60), (160, 80, 30)),
    }
    primary, secondary = colors.get(kind, ((200, 200, 200), (150, 150, 150)))

    if kind == "holy_water":
        # Bottle shape
        rect(draw, bx + 6, by + 3, 4, 2, (200, 200, 220))
        rect(draw, bx + 5, by + 5, 6, 8, primary)
        rect(draw, bx + 6, by + 6, 4, 6, secondary)
        px(draw, bx + 7, by + 7, (140, 200, 255))
    elif kind == "crucifix":
        rect(draw, bx + 7, by + 3, 2, 11, primary)
        rect(draw, bx + 4, by + 5, 8, 2, primary)
        px(draw, bx + 7, by + 5, secondary)
        px(draw, bx + 8, by + 5, secondary)
    elif kind == "speed_boots":
        rect(draw, bx + 4, by + 8, 8, 4, primary)
        rect(draw, bx + 3, by + 10, 3, 3, secondary)
        rect(draw, bx + 10, by + 10, 3, 3, secondary)
        # Lightning bolt
        px(draw, bx + 7, by + 5, (255, 255, 100))
        px(draw, bx + 8, by + 6, (255, 255, 100))
        px(draw, bx + 7, by + 7, (255, 255, 100))
    elif kind == "double_jump":
        # Wings
        rect(draw, bx + 3, by + 6, 4, 3, primary)
        rect(draw, bx + 9, by + 6, 4, 3, primary)
        rect(draw, bx + 6, by + 8, 4, 4, secondary)
        px(draw, bx + 4, by + 5, primary)
        px(draw, bx + 11, by + 5, primary)
    elif kind == "armor":
        rect(draw, bx + 5, by + 4, 6, 8, primary)
        rect(draw, bx + 6, by + 5, 4, 6, secondary)
        rect(draw, bx + 5, by + 4, 6, 1, (200, 200, 210))
    elif kind == "invincibility":
        # Star shape
        rect(draw, bx + 6, by + 3, 4, 10, primary)
        rect(draw, bx + 3, by + 5, 10, 4, primary)
        px(draw, bx + 7, by + 6, secondary)
        px(draw, bx + 8, by + 6, secondary)
        px(draw, bx + 7, by + 7, (255, 255, 200))
        px(draw, bx + 8, by + 7, (255, 255, 200))
    elif kind == "whip_extend":
        # Coiled whip
        for i in range(6):
            rect(draw, bx + 4 + i, by + 6 + (i % 3), 2, 2, primary)
        rect(draw, bx + 10, by + 5, 2, 4, secondary)


def draw_heart(draw, bx, by, full):
    """Pixel heart icon."""
    color = (220, 40, 40) if full else (80, 60, 60)
    shadow = (160, 20, 20) if full else (60, 45, 45)
    # Heart shape
    rect(draw, bx + 2, by + 4, 4, 3, color)
    rect(draw, bx + 10, by + 4, 4, 3, color)
    rect(draw, bx + 1, by + 5, 14, 4, color)
    rect(draw, bx + 2, by + 9, 12, 2, color)
    rect(draw, bx + 3, by + 11, 10, 1, color)
    rect(draw, bx + 4, by + 12, 8, 1, color)
    rect(draw, bx + 5, by + 13, 6, 1, shadow)
    rect(draw, bx + 6, by + 14, 4, 1, shadow)
    # Highlight
    if full:
        px(draw, bx + 4, by + 5, (255, 120, 120))
        px(draw, bx + 5, by + 5, (255, 160, 160))


def draw_prop(draw, bx, by, kind):
    """Decorative props."""
    if kind == "candelabra":
        rect(draw, bx + 7, by + 6, 2, 10, PAL["knight_gold"])
        rect(draw, bx + 4, by + 5, 8, 2, PAL["knight_gold"])
        # Candles
        for cx in [bx + 4, bx + 7, bx + 10]:
            rect(draw, cx, by + 2, 2, 4, (230, 220, 200))
            px(draw, cx, by + 1, PAL["fire_bright"])
    elif kind == "cross":
        rect(draw, bx + 7, by + 3, 2, 12, PAL["stone_light"])
        rect(draw, bx + 4, by + 5, 8, 2, PAL["stone_light"])
    elif kind == "gravestone":
        rect(draw, bx + 3, by + 6, 10, 10, PAL["stone_mid"])
        rect(draw, bx + 4, by + 3, 8, 4, PAL["stone_mid"])
        rect(draw, bx + 5, by + 2, 6, 2, PAL["stone_light"])
        # RIP text hint
        px(draw, bx + 6, by + 8, PAL["stone_dark"])
        px(draw, bx + 8, by + 8, PAL["stone_dark"])
        px(draw, bx + 10, by + 8, PAL["stone_dark"])


# ── Particle drawing ──────────────────────────────────────────────

def draw_particle(draw, bx, by, kind, frame):
    """Draw 8x8 particle sprites."""
    if kind == "dust":
        alpha = [200, 160, 120, 80][frame % 4]
        size = [3, 4, 3, 2][frame % 4]
        cx = bx + 4 - size // 2
        cy = by + 4 - size // 2
        rect(draw, cx, cy, size, size, (180, 170, 150), alpha)
    elif kind == "spark":
        alpha = [255, 200, 140][frame % 3]
        rect(draw, bx + 3, by + 3, 2, 2, (255, 240, 100), alpha)
        px(draw, bx + 4, by + 2, (255, 255, 200), alpha)
        px(draw, bx + 2, by + 4, (255, 255, 200), alpha)
    elif kind == "blood":
        alpha = [255, 200, 140][frame % 3]
        size = [3, 2, 2][frame % 3]
        rect(draw, bx + 3, by + 3, size, size, (180, 20, 20), alpha)
    elif kind == "fire":
        colors = [(255, 220, 80), (240, 160, 40), (220, 100, 20), (180, 60, 10)]
        c = colors[frame % 4]
        h = [4, 5, 4, 3][frame % 4]
        rect(draw, bx + 2, by + 8 - h, 4, h, c, 230)
        px(draw, bx + 3, by + 8 - h - 1, (255, 255, 200), 180)
    elif kind == "magic":
        colors = [(100, 255, 150), (80, 200, 255), (200, 150, 255)]
        c = colors[frame % 3]
        rect(draw, bx + 2, by + 2, 4, 4, c, 200)
        px(draw, bx + 3, by + 3, (255, 255, 255), 180)
    elif kind == "smoke":
        alpha = [160, 120, 80][frame % 3]
        size = [3, 4, 5][frame % 3]
        cx = bx + 4 - size // 2
        cy = by + 4 - size // 2
        rect(draw, cx, cy, size, size, (120, 110, 130), alpha)
    elif kind == "debris":
        alpha = [255, 200, 140][frame % 3]
        rect(draw, bx + 2, by + 3, 3, 3, PAL["stone_mid"], alpha)
        px(draw, bx + 3, by + 2, PAL["stone_light"], alpha)
    elif kind == "water":
        alpha = [200, 160, 100][frame % 3]
        rect(draw, bx + 2, by + 2, 3, 4, PAL["water_surface"], alpha)
        px(draw, bx + 3, by + 1, PAL["water_highlight"], alpha)
    elif kind == "ember":
        colors = [(255, 200, 60), (255, 160, 40), (200, 100, 20)]
        c = colors[frame % 3]
        px(draw, bx + 3, by + 3, c, 255)
        px(draw, bx + 4, by + 4, c, 180)


# ═══════════════════════════════════════════════════════════════════
# Atlas builder
# ═══════════════════════════════════════════════════════════════════

def build_atlas():
    """Build the 1024x512 sprite atlas with GothicVania-style pixel art."""
    atlas = Image.new("RGBA", (ATLAS_W, ATLAS_H), (0, 0, 0, 0))
    draw = ImageDraw.Draw(atlas)
    metadata = {}

    random.seed(42)  # Deterministic

    # Draw all sprites with proper pixel art
    for name, x, y, w, h in SPRITES:
        draw_gothic_sprite(draw, name, x, y, w, h)
        metadata[name] = {"x": x, "y": y, "w": w, "h": h}

    # Legacy aliases
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
    output = {"atlas_width": ATLAS_W, "atlas_height": ATLAS_H, "sprites": metadata, "legacy_aliases": legacy_map}
    with open(json_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"Saved metadata: {json_path} ({len(metadata)} sprites)")

    return atlas, metadata


def draw_gothic_sprite(draw, name, x, y, w, h):
    """Route sprite drawing to the appropriate handler."""
    # Extract frame number from name
    parts = name.rsplit("_", 1)
    frame = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 0

    # Player sprites
    if name.startswith("player_idle"):
        draw_player_idle(draw, x, y, frame)
    elif name.startswith("player_walk"):
        draw_player_walk(draw, x, y, frame)
    elif name.startswith("player_jump"):
        draw_player_jump(draw, x, y, frame)
    elif name.startswith("player_fall"):
        draw_player_fall(draw, x, y, frame)
    elif name.startswith("player_attack"):
        draw_player_attack(draw, x, y, frame)
    elif name.startswith("player_hurt"):
        draw_player_hurt(draw, x, y, frame)
    elif name.startswith("player_dead"):
        draw_player_dead(draw, x, y, frame)
    # Enemy sprites
    elif name.startswith("skeleton_walk"):
        draw_skeleton(draw, x, y, frame, "walk")
    elif name.startswith("skeleton_attack"):
        draw_skeleton(draw, x, y, frame, "attack")
    elif name.startswith("skeleton_death"):
        draw_skeleton(draw, x, y, frame, "death")
    elif name.startswith("bat_fly"):
        draw_bat(draw, x, y, frame, alive=True)
    elif name.startswith("bat_death"):
        draw_bat(draw, x, y, frame, alive=False)
    elif name.startswith("knight_walk"):
        draw_knight(draw, x, y, frame, "walk")
    elif name.startswith("knight_attack"):
        draw_knight(draw, x, y, frame, "attack")
    elif name.startswith("knight_death"):
        draw_knight(draw, x, y, frame, "death")
    elif name.startswith("medusa_float"):
        draw_medusa(draw, x, y, frame, alive=True)
    elif name.startswith("medusa_death"):
        draw_medusa(draw, x, y, frame, alive=False)
    elif name.startswith("projectile"):
        draw_projectile(draw, x, y, frame)
    # Tiles
    elif name.startswith("stone_brick"):
        draw_stone_brick(draw, x, y, name.replace("stone_brick_", ""))
    elif name.startswith("platform"):
        draw_platform(draw, x, y, frame)
    elif name.startswith("spikes"):
        draw_spikes(draw, x, y, frame)
    elif name.startswith("checkpoint_flag_down"):
        draw_checkpoint_flag(draw, x, y, frame, activated=False)
    elif name.startswith("checkpoint_flag_up"):
        draw_checkpoint_flag(draw, x, y, frame, activated=True)
    elif name.startswith("finish_gate"):
        draw_finish_gate(draw, x, y, frame)
    elif name == "ladder":
        draw_ladder(draw, x, y)
    elif name.startswith("breakable_wall"):
        draw_breakable_wall(draw, x, y, frame)
    elif name.startswith("torch_"):
        draw_torch(draw, x, y, frame)
    elif name == "stained_glass":
        draw_stained_glass(draw, x, y)
    elif name == "water_surface":
        draw_water_tile(draw, x, y, is_surface=True)
    elif name == "water_body":
        draw_water_tile(draw, x, y, is_surface=False)
    elif name == "cobweb":
        draw_cobweb(draw, x, y)
    elif name.startswith("chain_"):
        draw_chain(draw, x, y, frame)
    # Power-ups
    elif name.startswith("powerup_"):
        kind = name.replace("powerup_", "")
        draw_powerup(draw, x, y, kind)
    # Hearts
    elif name == "heart_full":
        draw_heart(draw, x, y, full=True)
    elif name == "heart_empty":
        draw_heart(draw, x, y, full=False)
    # Props
    elif name.startswith("prop_"):
        draw_prop(draw, x, y, name.replace("prop_", ""))
    # Particles
    elif name.startswith("particle_"):
        kind = name.rsplit("_", 1)[0].replace("particle_", "")
        draw_particle(draw, x, y, kind, frame)


def draw_projectile(draw, bx, by, frame):
    """Enemy projectile: spinning energy orb."""
    colors = [(200, 80, 200), (220, 100, 220), (180, 60, 180)]
    c = colors[frame % 3]
    # Orb
    rect(draw, bx + 4, by + 4, 8, 8, c)
    rect(draw, bx + 5, by + 5, 6, 6, (240, 140, 240))
    # Core glow
    rect(draw, bx + 6, by + 6, 4, 4, (255, 200, 255))
    # Spark trails
    px(draw, bx + 3, by + 7, c, 180)
    px(draw, bx + 12, by + 7, c, 180)


def build_background():
    """Build the 512x512 parallax background (3 layers stacked vertically)."""
    BG_SIZE = 512
    bg = Image.new("RGBA", (BG_SIZE, BG_SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(bg)
    random.seed(42)

    layer_h = BG_SIZE // 3

    # Layer 0 (sky): deep gothic night sky
    for y in range(layer_h):
        t = y / layer_h
        r = int(12 + t * 18)
        g = int(8 + t * 15)
        b = int(30 + t * 35)
        draw.line([(0, y), (BG_SIZE - 1, y)], fill=(r, g, b, 255))
    # Stars
    for _ in range(60):
        sx = random.randint(0, BG_SIZE - 1)
        sy = random.randint(0, layer_h - 1)
        brightness = random.randint(120, 255)
        size = random.choice([1, 1, 1, 2])
        if size == 1:
            draw.point((sx, sy), fill=(brightness, brightness, brightness, 255))
        else:
            draw.point((sx, sy), fill=(brightness, brightness, brightness, 255))
            draw.point((sx + 1, sy), fill=(brightness, brightness, brightness, 180))
            draw.point((sx, sy + 1), fill=(brightness, brightness, brightness, 180))
    # Moon
    for dy in range(-8, 9):
        for dx in range(-8, 9):
            if dx * dx + dy * dy <= 64:
                alpha = max(0, 255 - (dx * dx + dy * dy) * 3)
                draw.point(
                    (400 + dx, 30 + dy),
                    fill=(220, 210, 180, alpha),
                )

    # Layer 1 (mid-ground): gothic castle silhouettes
    y_base = layer_h
    for y in range(layer_h):
        t = y / layer_h
        r = int(20 + t * 20)
        g = int(15 + t * 15)
        b = int(35 + t * 25)
        draw.line([(0, y_base + y), (BG_SIZE - 1, y_base + y)], fill=(r, g, b, 255))
    # Distant castle silhouettes
    castle_color = (15, 10, 25, 255)
    # Large castle
    draw.rectangle([80, y_base + 60, 180, y_base + layer_h], fill=castle_color)
    draw.rectangle([90, y_base + 40, 110, y_base + 65], fill=castle_color)
    draw.rectangle([140, y_base + 45, 165, y_base + 65], fill=castle_color)
    # Towers with pointed tops
    for tx in [95, 100, 145, 155]:
        draw.polygon(
            [(tx - 3, y_base + 40), (tx + 3, y_base + 40), (tx, y_base + 30)],
            fill=castle_color,
        )
    # Distant hills
    for x in range(BG_SIZE):
        hill_h = int(25 + 18 * math.sin(x * 0.015) + 12 * math.sin(x * 0.04 + 0.7))
        for y in range(hill_h):
            py = y_base + layer_h - 1 - y
            if y_base <= py < y_base + layer_h:
                draw.point((x, py), fill=(18, 12, 30, 255))
    # Window lights on castle
    for wx in range(90, 170, 12):
        for wy in range(y_base + 70, y_base + layer_h - 10, 15):
            if random.random() > 0.5:
                draw.rectangle(
                    [wx, wy, wx + 3, wy + 4],
                    fill=(200, 170, 60, 140),
                )

    # Layer 2 (near-ground): cemetery/church buildings
    y_base = layer_h * 2
    for y in range(layer_h):
        t = y / layer_h
        r = int(30 + t * 25)
        g = int(22 + t * 18)
        b = int(45 + t * 25)
        draw.line([(0, y_base + y), (BG_SIZE - 1, y_base + y)], fill=(r, g, b, 255))
    bldg_color = (12, 8, 20, 255)
    # Buildings with varied heights
    bx = 0
    while bx < BG_SIZE:
        bw = random.randint(20, 45)
        bh = random.randint(40, 80)
        draw.rectangle([bx, y_base + layer_h - bh, bx + bw, y_base + layer_h], fill=bldg_color)
        # Pointed roof
        if random.random() > 0.4:
            draw.polygon(
                [
                    (bx, y_base + layer_h - bh),
                    (bx + bw, y_base + layer_h - bh),
                    (bx + bw // 2, y_base + layer_h - bh - 15),
                ],
                fill=bldg_color,
            )
        # Windows
        for wx in range(bx + 4, bx + bw - 4, 8):
            for wy_off in range(10, bh - 8, 12):
                if random.random() > 0.35:
                    wy = y_base + layer_h - bh + wy_off
                    draw.rectangle(
                        [wx, wy, wx + 3, wy + 5],
                        fill=(220, 190, 80, 160),
                    )
        # Cross on some buildings
        if random.random() > 0.6:
            cx = bx + bw // 2
            cy = y_base + layer_h - bh - 5
            draw.rectangle([cx - 1, cy - 4, cx + 1, cy + 2], fill=bldg_color)
            draw.rectangle([cx - 3, cy - 2, cx + 3, cy], fill=bldg_color)
        bx += bw + random.randint(2, 10)

    # Ground fog strip at bottom of layer 2
    for x in range(BG_SIZE):
        for y in range(8):
            alpha = int(80 * (1 - y / 8))
            py = y_base + layer_h - 1 - y
            draw.point((x, py), fill=(60, 50, 70, alpha))

    bg_path = os.path.join(OUT_DIR, "platformer_bg.png")
    bg.save(bg_path, "PNG")
    print(f"Saved background: {bg_path} ({bg.size[0]}x{bg.size[1]})")


if __name__ == "__main__":
    build_atlas()
    build_background()
    print("Done!")
