from pathlib import Path

root = Path.cwd()
application = root / 'release/native/ViGo.app'
files = [str(application)]
symlinks = {'应用程序': '/Applications'}
# Never change FinderInfo on a signed bundle; Finder hides .app using its own preference.
hide_extensions = []
icon = str(root / 'release/dmg-volume.icns')
background = str(root / 'release/dmg-background.tiff')
icon_locations = {'ViGo.app': (166, 192), '应用程序': (474, 192)}
window_rect = ((180, 180), (640, 400))
default_view = 'icon-view'
show_toolbar = False
show_sidebar = False
show_status_bar = False
show_pathbar = False
show_tab_view = False
include_icon_view_settings = True
include_list_view_settings = False
icon_size = 96
text_size = 14
format = 'UDZO'
filesystem = 'HFS+'
