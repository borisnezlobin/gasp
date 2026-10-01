# dmgbuild settings for Gasp's install window. scripts/package-macos.sh passes the app with
#   dmgbuild -s scripts/dmg-settings.py -D app=target/package/Gasp.app -D background=... Gasp out.dmg
# The window size and icon spots match the art drawn by scripts/dmg-background.py.
import os.path

app = defines["app"]  # noqa: F821
app_name = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"

files = [app]
symlinks = {"Applications": "/Applications"}

background = defines["background"]  # noqa: F821
window_rect = ((200, 200), (660, 420))
default_view = "icon-view"
show_toolbar = False
show_sidebar = False
show_status_bar = False
show_tab_view = False
show_pathbar = False
show_icon_preview = False

icon_size = 128
text_size = 13
label_pos = "bottom"
arrange_by = None
icon_locations = {
    app_name: (170, 190),
    "Applications": (490, 190),
}
