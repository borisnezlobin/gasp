"""Renders the icon's humpback in the ink style on a transparent background.

The whale comes from the humpback kit (see THIRD_PARTY_NOTICES.txt), which has
the rigged model and the ink material. Run it with the kit's Blender file:

    blender -b <kit>/dist/humpback-whale.blend --python render_whale.py -- <kit>/scripts whale-breach.png

The result is committed as whale-breach.png, so building the icon doesn't need
Blender or the kit.
"""
import sys

import bpy

ACTION = "breach"
FRAME = 14
WIDTH = 3200
HEIGHT = 2560
SAMPLES = 64

kit_scripts, output = sys.argv[sys.argv.index("--") + 1:][:2]
sys.path.insert(0, kit_scripts)
from ink_style import scene as ink_scene

scene = bpy.context.scene
armature = bpy.data.objects["humpback_rig"]
action = bpy.data.actions[ACTION]
armature.animation_data_create()
armature.animation_data.action = action
if hasattr(armature.animation_data, "action_slot") and action.slots:
    armature.animation_data.action_slot = action.slots[0]

ink_scene.prepare(scene, bpy.data.objects["humpback_whale"], paper=False, composite=False)
scene.render.resolution_x = WIDTH
scene.render.resolution_y = HEIGHT
scene.eevee.taa_render_samples = SAMPLES
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
scene.frame_set(FRAME)
scene.render.filepath = output
bpy.ops.render.render(write_still=True)
